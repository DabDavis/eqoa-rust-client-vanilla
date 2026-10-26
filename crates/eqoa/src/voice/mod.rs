//! Voice chat (the user, F3 pass 18: "page and native together", push-to-talk
//! by default with open mic as the option, "off until turned on", private: no
//! player sees another's address), the client's half of go-eqoa-server's
//! voice (`net::voice`, 0x7E10-0x7E16): with Voice Chat on (Extended > Sound)
//! the server is told so and sends a fallback relay and the roster of whom the
//! player may hear (its group, and the lobby room it is in: rooms anyone
//! makes, open, locked by a password the server never sees, or unlisted), and
//! carries the WebRTC signals between the client and its forwarding server,
//! the ONE connection the voice goes over. On a server that offers no voice
//! (Sandstorm), Voice Demo uses eqoaclassic's voice server directly
//! (`demo`). The platform's engine (`api::VoiceEngine`: a page's WebRTC,
//! `web::voice`; natively crate `voice`'s) owns the microphone and the
//! connection; this decides what is sent and how each voice is heard: the
//! group's push-to-talk key (V) sends to the group, the room's (B) to the
//! room, Open Mic to both while the microphone hears speech; every voice at
//! Voice Volume times its member's own, a muted member at none. Nothing is
//! asked of the microphone, and nothing told the server, while Voice Chat is
//! off.

pub mod api;
pub mod command;
pub mod demo;
pub mod ids;
pub mod view;

pub use api::{IceServer, NullEngine, SignalKind, VoiceEngine, VoiceEvent};
pub use view::{VoiceMember, VoiceMode, VoiceRoomInfo, VoiceView};

use crate::settings::VoiceChat;
use net::voice::{CHANNEL_GROUP, CHANNEL_ROOM, FLAG_ON, ROOM_IN, ROOM_LEAVE, ROOM_LEFT, ROOM_LIST, ROOM_REFUSED, ROOM_WRONG_PASSWORD, WireIce, WireMember, WireRoom};
use std::collections::{HashMap, HashSet};

/// A level the marks show as speaking (about -39 dBFS, `api`'s scale).
pub const SPEAKING: f32 = 0.35;
/// Open Mic's voice detection: the microphone above this (about -35 dBFS)
/// opens it, and it stays open this long after.
const GATE_OPEN: f32 = 0.42;
const GATE_HOLD_MS: i64 = 600;
/// How long into the world a server that has not offered voice is taken to
/// have none (its offer comes with the world entry's first 0x0014).
const NO_OFFER_MS: i64 = 10_000;
/// The waits before the connection is tried again after it was lost.
const RETRY_MS: [i64; 4] = [5_000, 10_000, 20_000, 60_000];
/// How long a room's line waits for the roster that has the room in it.
const ROOM_LINE_MS: i64 = 1_500;

/// Where voice's signaling goes: the game connection (0x7E10-0x7E16), or the
/// demo's socket (`demo::Demo`).
pub trait Link {
    /// Voice on or off, told the server once and on a change.
    fn agree(&mut self, on: bool);
    /// Whether the server has voice (offered it; the demo's: always).
    fn offered(&self) -> bool;
    /// The fallback ICE servers once the server answered voice on.
    fn ice(&self) -> Option<Vec<WireIce>>;
    /// Whom the player may hear, with what joins them (`CHANNEL_*`).
    fn roster(&self) -> Vec<WireMember>;
    fn take_signals(&mut self) -> Vec<(u8, String)>;
    fn send_signal(&mut self, kind: u8, text: &str);
    /// The lobby: a room joined (its H; unlisted when the join makes it),
    /// left (`ROOM_LEAVE`), listed (`ROOM_LIST`).
    fn room_join(&mut self, name: &str, hash: &str, unlisted: bool);
    fn room_op(&mut self, op: u8);
    /// A lobby the server sent since the last call.
    fn take_lobby(&mut self) -> Option<Vec<WireRoom>>;
    /// The rooms' answers since the last call.
    fn take_room_states(&mut self) -> Vec<(u8, String)>;
    /// Lines of the link's own since the last call.
    fn take_lines(&mut self) -> Vec<String> {
        Vec::new()
    }
}

impl Link for net::session::Session {
    fn agree(&mut self, on: bool) {
        self.agree_voice(if on { FLAG_ON } else { 0 });
    }
    fn offered(&self) -> bool {
        self.voice_offer
    }
    fn ice(&self) -> Option<Vec<WireIce>> {
        self.voice_ice.clone()
    }
    fn roster(&self) -> Vec<WireMember> {
        self.voice_roster.clone()
    }
    fn take_signals(&mut self) -> Vec<(u8, String)> {
        self.take_voice_signals()
    }
    fn send_signal(&mut self, kind: u8, text: &str) {
        self.send_voice_signal(kind, text);
    }
    fn room_join(&mut self, name: &str, hash: &str, unlisted: bool) {
        self.voice_room_join(name, hash, unlisted);
    }
    fn room_op(&mut self, op: u8) {
        self.voice_room_op(op);
    }
    fn take_lobby(&mut self) -> Option<Vec<WireRoom>> {
        self.voice_lobby.take()
    }
    fn take_room_states(&mut self) -> Vec<(u8, String)> {
        self.take_voice_room_states()
    }
}

/// Play's voice: the manager, the demo's link, and the push-to-talk keys held
/// (the group's, the room's: the viewer's, each frame).
pub struct PlayVoice {
    pub voice: Voice,
    pub demo: demo::Demo,
    pub keys: [bool; 2],
}

impl PlayVoice {
    pub fn for_platform() -> Self {
        PlayVoice { voice: Voice::for_platform(), demo: demo::Demo::for_platform(), keys: [false; 2] }
    }
}

/// What the player asks of voice this frame.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Wants {
    pub chat: VoiceChat,
    /// Voice Demo: eqoaclassic's where the server offers none
    pub demo: bool,
    /// the push-to-talk keys held: the group's, the room's
    pub keys: [bool; 2],
    /// Voice Volume, in tenths
    pub volume: u8,
}

/// A lobby op asked for, carried out on the next frame's link.
#[derive(Clone, PartialEq, Eq)]
enum Op {
    Join { name: String, hash: String, unlisted: bool },
    Leave,
    List,
}

/// A member's mark as the HUD draws it.
#[derive(Clone, Debug, PartialEq)]
pub struct Mark {
    pub peer: u32,
    pub name: String,
    pub level: f32,
    pub muted: bool,
    pub group: bool,
    pub room: bool,
}

/// The room the player is in, as the client knows it.
#[derive(Clone, Debug, PartialEq, Eq)]
struct Room {
    name: String,
    locked: bool,
    unlisted: bool,
}

/// Voice's state between frames.
pub struct Voice {
    engine: Box<dyn VoiceEngine>,
    /// whether the platform has voice media at all (natively not until crate
    /// `voice`'s engine is in)
    has_media: bool,
    started: bool,
    connected: bool,
    ever_connected: bool,
    told_lost: bool,
    retry_at: i64,
    retries: usize,
    /// the link in use last frame: the demo's or the game's
    on_demo: Option<bool>,
    ice: Vec<WireIce>,
    /// what is sent (`CHANNEL_*`) and what the engine was last told
    sending: u8,
    send_told: Option<u8>,
    gate_until: i64,
    roster: Vec<WireMember>,
    roster_seen: bool,
    roster_at: i64,
    /// each member's volume as last told the engine, and as the player set
    /// it (0..2, by folded name), the members muted (by folded name)
    volumes_told: HashMap<u32, f32>,
    member_volume: HashMap<String, f32>,
    muted: HashSet<String>,
    levels: Vec<(u32, f32)>,
    mic: f32,
    room: Option<Room>,
    /// a join asked for and not yet answered; the room's line waiting for its
    /// roster
    joining: Option<Room>,
    room_line_at: Option<i64>,
    lobby: Vec<WireRoom>,
    lobby_asked: bool,
    ops: Vec<Op>,
    lines: Vec<String>,
    status: String,
    first_frame: Option<i64>,
    told_none: bool,
    told_demo: bool,
    told_no_media: bool,
    /// the last frame's: Voice Chat on, the server offering voice
    was_on: bool,
    offered: bool,
}

impl Voice {
    pub fn new(engine: Box<dyn VoiceEngine>, has_media: bool) -> Self {
        Voice {
            engine,
            has_media,
            started: false,
            connected: false,
            ever_connected: false,
            told_lost: false,
            retry_at: 0,
            retries: 0,
            on_demo: None,
            ice: Vec::new(),
            sending: 0,
            send_told: None,
            gate_until: 0,
            roster: Vec::new(),
            roster_seen: false,
            roster_at: 0,
            volumes_told: HashMap::new(),
            member_volume: HashMap::new(),
            muted: HashSet::new(),
            levels: Vec::new(),
            mic: 0.0,
            room: None,
            joining: None,
            room_line_at: None,
            lobby: Vec::new(),
            lobby_asked: false,
            ops: Vec::new(),
            lines: Vec::new(),
            status: String::new(),
            first_frame: None,
            told_none: false,
            told_demo: false,
            told_no_media: false,
            was_on: false,
            offered: false,
        }
    }

    /// The platform's: a page's WebRTC; natively none yet (crate `voice`'s
    /// engine is lane voicenat18's).
    pub fn for_platform() -> Self {
        let (engine, media) = platform_engine();
        Voice::new(engine, media)
    }

    fn say(&mut self, line: String) {
        self.status = line.clone();
        self.lines.push(line);
    }

    /// A frame's voice, on the game's link or the demo's: the server told the
    /// player's choice; the lobby's answers and the roster turned into lines;
    /// the engine started once the server answered (again a while after the
    /// connection was lost), the signals both ways, what is sent and how loud
    /// each voice is; off, all of it let go.
    pub fn frame(&mut self, game: &mut dyn Link, demo: &mut dyn Link, w: Wants, now: i64) {
        let first = *self.first_frame.get_or_insert(now);
        let on = w.chat != VoiceChat::Off && self.has_media;
        self.offered = game.offered();
        let none_offered = !self.offered && now - first >= NO_OFFER_MS;
        let use_demo = on && w.demo && none_offered;
        let (link, other): (&mut dyn Link, &mut dyn Link) = if use_demo { (demo, game) } else { (game, demo) };
        other.agree(false);
        if self.on_demo.is_some_and(|d| d != use_demo) {
            self.leave_all();
        }
        self.on_demo = Some(use_demo);
        link.agree(on);
        if w.chat != VoiceChat::Off && !self.has_media && !self.told_no_media {
            self.told_no_media = true;
            self.say("Voice chat is coming to this build; the page has it now (eqoaclassic.com/eqoa/play/).".to_string());
        }
        if !on {
            if self.was_on {
                self.leave_all();
            }
            self.was_on = false;
            self.ops.clear();
            return;
        }
        self.was_on = true;
        if !link.offered() {
            if none_offered && !self.told_none {
                self.told_none = true;
                self.say("Voice Chat: this server has no voice chat (Extended > Sound > Voice Demo uses eqoaclassic's).".to_string());
            }
            return;
        }
        if use_demo && !self.told_demo {
            self.told_demo = true;
            self.say("Voice demo: eqoaclassic's voice server. Anyone who knows your group's names, or a room's name, can join you there.".to_string());
        }
        for l in link.take_lines() {
            self.say(l);
        }
        // the lobby
        for op in std::mem::take(&mut self.ops) {
            match op {
                Op::Join { name, hash, unlisted } => link.room_join(&name, &hash, unlisted),
                Op::Leave => link.room_op(ROOM_LEAVE),
                Op::List => link.room_op(ROOM_LIST),
            }
        }
        if let Some(l) = link.take_lobby() {
            self.lobby = l;
            if std::mem::take(&mut self.lobby_asked) {
                self.lobby_lines();
            }
        }
        for (result, name) in link.take_room_states() {
            self.room_state(result, &name, now);
        }
        // the roster
        let roster = link.roster();
        if roster != self.roster {
            self.roster_lines(&roster);
            self.roster = roster;
            self.roster_at = now;
        }
        if let Some(at) = self.room_line_at
            && (self.roster_at > at || now - at >= ROOM_LINE_MS)
        {
            self.room_line_at = None;
            self.room_line();
        }
        // the connection
        if let Some(ice) = link.ice() {
            self.ice = ice;
            if !self.started && now >= self.retry_at {
                let ice: Vec<IceServer> = self.ice.iter().map(|i| IceServer { url: i.url.clone(), username: i.username.clone(), credential: i.credential.clone() }).collect();
                self.engine.start(&ice);
                self.started = true;
                self.send_told = None;
                self.volumes_told.clear();
            }
        }
        if !self.started {
            return;
        }
        for (kind, text) in link.take_signals() {
            match SignalKind::of(kind) {
                Some(SignalKind::Bye) => {
                    self.lost(now);
                    return;
                }
                Some(k) => self.engine.signal(k, &text),
                None => {}
            }
        }
        for (k, text) in self.engine.take_outgoing() {
            link.send_signal(k.byte(), &text);
        }
        // what is sent: the group's key the group, the room's the room; open
        // mic both while it hears speech
        if self.mic > GATE_OPEN {
            self.gate_until = now + GATE_HOLD_MS;
        }
        let in_room = if self.room.is_some() { CHANNEL_ROOM } else { 0 };
        self.sending = match w.chat {
            VoiceChat::Off => 0,
            VoiceChat::PushToTalk => (if w.keys[0] { CHANNEL_GROUP } else { 0 }) | (if w.keys[1] { in_room } else { 0 }),
            VoiceChat::OpenMic => {
                if now < self.gate_until || w.keys[0] || w.keys[1] {
                    CHANNEL_GROUP | in_room
                } else {
                    0
                }
            }
        };
        if self.send_told != Some(self.sending) {
            self.send_told = Some(self.sending);
            self.engine.set_send(self.sending);
        }
        for m in &self.roster {
            let key = ids::fold(&m.name);
            let v = if self.muted.contains(&key) { 0.0 } else { self.member_volume.get(&key).copied().unwrap_or(1.0) * w.volume.min(crate::settings::VOICE_VOLUME_MAX) as f32 / 10.0 };
            if self.volumes_told.get(&m.peer) != Some(&v) {
                self.volumes_told.insert(m.peer, v);
                self.engine.set_peer_volume(m.peer, v);
            }
        }
        for e in self.engine.take_events() {
            match e {
                VoiceEvent::Connected => {
                    self.connected = true;
                    self.retries = 0;
                    self.told_lost = false;
                    let again = std::mem::replace(&mut self.ever_connected, true);
                    self.say(if again { "Voice chat connected again." } else { "Voice chat connected." }.to_string());
                }
                VoiceEvent::Failed => {
                    self.lost(now);
                    return;
                }
                VoiceEvent::MicDenied => self.say("Voice Chat: the microphone was not allowed, so no one hears you (you still hear them).".to_string()),
                VoiceEvent::Note(t) => self.say(t),
            }
        }
        self.levels = self.engine.levels();
        self.mic = self.engine.mic_level();
    }

    /// The connection lost: the engine stopped, tried again after a wait
    /// that grows; said once a streak.
    fn lost(&mut self, now: i64) {
        self.engine.stop();
        self.started = false;
        self.connected = false;
        self.levels.clear();
        self.mic = 0.0;
        self.send_told = None;
        self.retry_at = now + RETRY_MS[self.retries.min(RETRY_MS.len() - 1)];
        self.retries += 1;
        if !self.told_lost {
            self.told_lost = true;
            self.say(if self.ever_connected { "Voice chat lost its connection; trying again." } else { "Voice chat could not connect; trying again." }.to_string());
        }
    }

    /// Voice off, or the link changed: the engine stopped, the roster, the
    /// room and the lobby forgotten (the server forgets the room with voice).
    fn leave_all(&mut self) {
        if self.started {
            self.engine.stop();
        }
        if let Some(r) = self.room.take() {
            self.say(format!("Voice: left room {}.", r.name));
        }
        self.started = false;
        self.connected = false;
        self.told_lost = false;
        self.retries = 0;
        self.retry_at = 0;
        self.ice.clear();
        self.roster.clear();
        self.roster_seen = false;
        self.volumes_told.clear();
        self.levels.clear();
        self.mic = 0.0;
        self.sending = 0;
        self.send_told = None;
        self.joining = None;
        self.room_line_at = None;
        self.lobby.clear();
        self.lobby_asked = false;
    }

    fn room_state(&mut self, result: u8, name: &str, now: i64) {
        match result {
            ROOM_IN => {
                let asked = self.joining.take().filter(|j| ids::fold(&j.name) == ids::fold(name));
                let (locked, unlisted) = asked.map_or((false, false), |j| (j.locked, j.unlisted));
                self.room = Some(Room { name: name.to_string(), locked, unlisted });
                self.room_line_at = Some(now);
            }
            // a refused join lets go of what was asked for that room only
            // (another asked after it is still coming)
            ROOM_WRONG_PASSWORD => {
                if self.joining.as_ref().is_some_and(|j| ids::fold(&j.name) == ids::fold(name)) {
                    self.joining = None;
                }
                self.say(format!("Voice: wrong password for {name}."));
            }
            ROOM_REFUSED => {
                if self.joining.as_ref().is_some_and(|j| ids::fold(&j.name) == ids::fold(name)) {
                    self.joining = None;
                }
                self.say("Voice: that name is refused.".to_string());
            }
            ROOM_LEFT => {
                // a nameless one: the demo's socket lost, the room with it
                let name = if name.is_empty() { self.room.as_ref().map(|r| r.name.clone()) } else { Some(name.to_string()) };
                self.room = None;
                self.room_line_at = None;
                if let Some(n) = name {
                    self.say(format!("Voice: left room {n}."));
                }
            }
            _ => {}
        }
    }

    /// The line for the room just joined: made (nobody else in it), or in it
    /// with the others the roster has.
    fn room_line(&mut self) {
        let Some(r) = self.room.clone() else { return };
        let others = self.roster.iter().filter(|m| m.channels & CHANNEL_ROOM != 0).count();
        let line = match (others, r.unlisted) {
            (0, true) => format!("Voice: made unlisted room {}: only those who know its name can find it.", r.name),
            (0, false) => format!("Voice: made room {}{}.", r.name, if r.locked { ", locked" } else { "" }),
            (1, _) => format!("Voice: in room {}, 1 other.", r.name),
            (n, _) => format!("Voice: in room {}, {n} others.", r.name),
        };
        self.say(line);
    }

    fn lobby_lines(&mut self) {
        if self.lobby.is_empty() {
            self.say("Voice: no rooms (/voice join <room> [password] makes one).".to_string());
            return;
        }
        let shown: Vec<String> = self.lobby.iter().take(12).map(|r| format!("{} ({}{})", r.name, r.members, if r.locked { ", locked" } else { "" })).collect();
        let more = self.lobby.len().saturating_sub(12);
        self.say(format!("Voice: rooms: {}{}.", shown.join(", "), if more > 0 { format!(" and {more} more") } else { String::new() }));
    }

    /// Lines for who came into voice and who left: the first roster in one
    /// line, then each change.
    fn roster_lines(&mut self, new: &[WireMember]) {
        let tag = |m: &WireMember| if m.channels & CHANNEL_GROUP != 0 { m.name.clone() } else { format!("{} (room)", m.name) };
        let list = |names: Vec<String>| {
            let n = names.len();
            if n <= 3 { names.join(", ") } else { format!("{} and {} more", names[..3].join(", "), n - 3) }
        };
        if !self.roster_seen {
            self.roster_seen = true;
            if !new.is_empty() {
                let names = new.iter().map(tag).collect();
                self.say(format!("Voice: with {}.", list(names)));
            }
            return;
        }
        let came: Vec<String> = new.iter().filter(|m| !self.roster.iter().any(|o| o.peer == m.peer)).map(tag).collect();
        let went: Vec<String> = self.roster.iter().filter(|o| !new.iter().any(|m| m.peer == o.peer)).map(tag).collect();
        if !came.is_empty() {
            self.say(format!("Voice: {} joined.", list(came)));
        }
        if !went.is_empty() {
            self.say(format!("Voice: {} left.", list(went)));
        }
    }

    /// The chat lines voice has for the player since the last call.
    pub fn take_lines(&mut self) -> Vec<String> {
        std::mem::take(&mut self.lines)
    }

    /// A `/voice` line (what follows the command), `on` Voice Chat's.
    pub fn command(&mut self, rest: &str, on: bool) {
        use command::Command;
        let c = command::parse(rest);
        if !matches!(c, Command::Usage | Command::BadName(_)) && !on {
            // the password (if any) dropped with `c`
            self.say("Voice: Voice Chat is off (Options, Extended, Sound).".to_string());
            return;
        }
        match c {
            Command::Rooms => self.refresh_lobby(),
            Command::Join { name, password, unlisted } => self.join(&name, password.as_deref().unwrap_or(""), unlisted),
            Command::Leave => self.leave(),
            Command::Who => self.who(),
            Command::BadName(_) => self.say(format!("Voice: a room's name is {} to {} letters.", command::NAME_MIN, command::NAME_MAX)),
            Command::Usage => self.say(command::USAGE.to_string()),
        }
    }

    /// The lobby asked for (`/voice rooms`, the window), said when it comes.
    pub fn refresh_lobby(&mut self) {
        self.ops.push(Op::List);
        self.lobby_asked = true;
    }

    /// Room `name` joined, made when it is not there (locked with
    /// `password`, "" none; unlisted): only its H goes on, the password
    /// scrubbed out of any report.
    pub fn join(&mut self, name: &str, password: &str, unlisted: bool) {
        if !command::good_name(name) {
            self.say(format!("Voice: a room's name is {} to {} letters.", command::NAME_MIN, command::NAME_MAX));
            return;
        }
        if !password.is_empty() {
            crate::report::secret("", password);
        }
        let name = name.trim().to_string();
        let hash = ids::room_hash(&name, Some(password));
        self.joining = Some(Room { name: name.clone(), locked: !hash.is_empty(), unlisted });
        self.ops.push(Op::Join { name, hash, unlisted });
    }

    pub fn leave(&mut self) {
        if self.room.is_none() && self.joining.is_none() {
            self.say("Voice: you are in no room.".to_string());
            return;
        }
        self.joining = None;
        self.ops.push(Op::Leave);
    }

    fn who(&mut self) {
        let Some(r) = self.room.clone() else {
            self.say("Voice: you are in no room (/voice rooms, /voice join <room> [password]).".to_string());
            return;
        };
        let names: Vec<String> = self.roster.iter().filter(|m| m.channels & CHANNEL_ROOM != 0).map(|m| m.name.clone()).collect();
        self.say(if names.is_empty() { format!("Voice: in room {}, alone.", r.name) } else { format!("Voice: in room {}: {}.", r.name, names.join(", ")) });
    }

    /// A member's own volume (0..2) and mute, kept by name for this run.
    pub fn set_volume(&mut self, peer: u32, volume: f32) {
        if let Some(m) = self.roster.iter().find(|m| m.peer == peer) {
            self.member_volume.insert(ids::fold(&m.name), volume.clamp(0.0, 2.0));
        }
    }

    pub fn set_muted(&mut self, peer: u32, muted: bool) {
        if let Some(m) = self.roster.iter().find(|m| m.peer == peer) {
            let key = ids::fold(&m.name);
            if muted {
                self.muted.insert(key);
            } else {
                self.muted.remove(&key);
            }
        }
    }

    /// Mutes or unmutes `peer` (a click on its mark): whether it is muted now.
    pub fn toggle_mute(&mut self, peer: u32) -> bool {
        let now = !self.marks().iter().any(|m| m.peer == peer && m.muted);
        self.set_muted(peer, now);
        now
    }

    /// Each member's mark.
    pub fn marks(&self) -> Vec<Mark> {
        self.roster
            .iter()
            .map(|m| Mark {
                peer: m.peer,
                name: m.name.clone(),
                level: self.levels.iter().find(|&&(q, _)| q == m.peer).map_or(0.0, |&(_, l)| l),
                muted: self.muted.contains(&ids::fold(&m.name)),
                group: m.channels & CHANNEL_GROUP != 0,
                room: m.channels & CHANNEL_ROOM != 0,
            })
            .collect()
    }

    /// The microphone's level while the player is heard, none while not.
    pub fn transmitting(&self) -> Option<f32> {
        (self.connected && self.sending != 0).then_some(self.mic)
    }

    /// Voice on and its connection up.
    pub fn connected(&self) -> bool {
        self.connected
    }

    /// Voice Chat on (last frame).
    pub fn on(&self) -> bool {
        self.was_on
    }

    /// The room's members speaking now who are not groupmates (whose bars
    /// show it), for the HUD's list.
    pub fn room_speakers(&self) -> Vec<Mark> {
        self.marks().into_iter().filter(|m| m.room && !m.group && m.level > SPEAKING).collect()
    }

    /// The window's view, `w` the player's choices now.
    pub fn view(&self, w: Wants, now: i64) -> VoiceView {
        let none_offered = !self.offered && self.first_frame.is_some_and(|f| now - f >= NO_OFFER_MS);
        let mode = match w.chat {
            VoiceChat::Off => VoiceMode::Off,
            _ if w.demo && none_offered => VoiceMode::Demo,
            VoiceChat::PushToTalk => VoiceMode::PushToTalk,
            VoiceChat::OpenMic => VoiceMode::OpenMic,
        };
        let fold = |n: &str| ids::fold(n);
        let listed = |name: &str| self.lobby.iter().any(|r| fold(&r.name) == fold(name));
        VoiceView {
            available: self.has_media,
            demo_only: none_offered,
            mode,
            connected: self.connected,
            mic_level: self.mic,
            transmitting: if self.connected { self.sending } else { 0 },
            room: self.room.as_ref().map(|r| (r.name.clone(), r.locked || self.lobby.iter().any(|l| l.locked && fold(&l.name) == fold(&r.name)), r.unlisted && !listed(&r.name))),
            members: self
                .marks()
                .into_iter()
                .map(|m| VoiceMember { peer: m.peer, volume: self.member_volume.get(&fold(&m.name)).copied().unwrap_or(1.0), name: m.name, group: m.group, room: m.room, level: m.level, muted: m.muted })
                .collect(),
            lobby: self.lobby.iter().map(|r| VoiceRoomInfo { name: r.name.clone(), members: r.members, locked: r.locked }).collect(),
            status: self.status.clone(),
        }
    }

    /// A report's line.
    pub fn describe(&self) -> String {
        if !self.was_on {
            return "voice: off".to_string();
        }
        format!(
            "voice: {}{}, {} in the roster, room {}, sending {}",
            if self.on_demo == Some(true) { "the demo, " } else { "" },
            if self.connected {
                "connected"
            } else if self.started {
                "connecting"
            } else {
                "not started"
            },
            self.roster.len(),
            self.room.as_ref().map_or("none", |r| r.name.as_str()),
            self.sending
        )
    }
}

/// The platform's engine and whether it carries voice: a page's WebRTC;
/// natively one that does nothing until crate `voice`'s is in.
fn platform_engine() -> (Box<dyn VoiceEngine>, bool) {
    #[cfg(target_arch = "wasm32")]
    {
        (Box::new(crate::web::voice::WebEngine::default()), true)
    }
    #[cfg(not(target_arch = "wasm32"))]
    {
        (Box::new(NullEngine::default()), false)
    }
}

#[cfg(test)]
mod tests;
