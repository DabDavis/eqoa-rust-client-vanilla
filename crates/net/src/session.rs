//! `VIClient`'s game connection (decomp `modules/client/src/viclient.cpp`):
//! DRDP over UDP, one opcode per message (`[u16 opcode][payload]`, sent as
//! type 251), routed on the DRDP message type the way `PumpMessages` does.
//!
//! The flow: `Connect` opens the login endpoint (drdp port 0xFFFE) and sends
//! the version hello and 0x0904; the server list 0x07B3 arrives on type 252;
//! `ConnectShard` opens the chosen game endpoint and sends the hello and
//! 0x0001; the character list 0x002C; `SelectCharacter` sends 0x002A; the
//! server then opens a NEW connection (a type-255 notice), which the client
//! adopts after resetting the world, and the MemoryDump 0x000D arrives on it
//! (its DRDP source is the player's own entity id), answered with 0x0014,
//! 0x00C9 and 0x00CA.

use crate::stream::{Reader, Writer};
use drdp::{Addr, ConnId, Drdp, PORT_LEADER, address_to_endpoint};
use std::net::{SocketAddr, UdpSocket};
use vi::fpu::Ee;
use web_time::{Instant, SystemTime, UNIX_EPOCH};

/// What the session sends its datagrams through: a UDP socket natively, a
/// relay's WebSocket in a browser. Addresses are IPv4 and a port.
pub trait Datagrams {
    fn send_to(&mut self, data: &[u8], ip: [u8; 4], port: u16);
    /// The next datagram waiting, if any, and its sender.
    fn recv_from(&mut self) -> Option<(Vec<u8>, [u8; 4], u16)>;
}

/// A non-blocking UDP socket.
pub struct Udp {
    socket: UdpSocket,
    buf: Vec<u8>,
}

impl Datagrams for Udp {
    fn send_to(&mut self, data: &[u8], ip: [u8; 4], port: u16) {
        let _ = self.socket.send_to(data, SocketAddr::from((ip, port)));
    }

    fn recv_from(&mut self) -> Option<(Vec<u8>, [u8; 4], u16)> {
        let (n, p) = self.socket.recv_from(&mut self.buf).ok()?;
        Some((self.buf[..n].to_vec(), ipv4(&p), p.port()))
    }
}

/// `VIClient`'s protocol version (0x0000 carries it both ways).
pub const VERSION: u32 = 18;
/// The expansion flags a retail Frontiers disc sends.
pub const EXPANSION_FLAGS: u32 = 3;
/// The group's chat lines' colour (0x069E: CLIENT's dispatcher adds them in
/// (.5, .5, 1)).
pub const GROUP_CHAT: [f32; 4] = [0.5, 0.5, 1.0, 1.0];
/// go-eqoa-server's own, no retail message: what a client keeps in view, how
/// many entity slots (`entity`) and how far, two u32s after the opcode, both
/// ways. The server offers the most it fills and the farthest it looks once
/// the player is in the world; the client answers with its NPC/PC Display and
/// NPC/PC View Distance, and again whenever they change. CLIENT has no case
/// for it (its dispatcher has no default), so a PS2 ignores the offer, and
/// nothing is sent to a server that made none.
pub const OP_ENTITY_VIEW: u16 = 0x7E00;
/// go-eqoa-server's own, no retail message: others' power, for the bars the
/// client draws under its group's, its target's and their target's health
/// (the game shows no one's but the player's own). A u32 after the opcode,
/// both ways: the server offers it (1) once the player is in the world, the
/// client answers 1 to have it and 0 to stop (`agree_power_bars`). A PS2
/// ignores the offer, as it does [`OP_ENTITY_VIEW`]'s; nothing is asked of a
/// server that made none.
pub const OP_POWER_BARS: u16 = 0x7E01;
/// The power, from a server the client asked ([`OP_POWER_BARS`]): a u8
/// count, then each a u32 id (as this client knows the entity) and a u8
/// power of 255. Each is the whole set: an id it leaves out has no bar.
pub const OP_POWER_BARS_DATA: u16 = 0x7E02;
/// go-eqoa-server's own, no retail message: the quest marks the map draws (a
/// "!" at each giver of a quest the player may take, a "?" where a step of
/// its quests is to be done). A u32 after the opcode, both ways, as
/// [`OP_POWER_BARS`]: offered once the player is in the world, answered 1 to
/// have them and 0 to stop (`agree_quest_marks`). A PS2 ignores the offer.
pub const OP_QUEST_MARKERS: u16 = 0x7E03;
/// The marks, from a server the client asked ([`OP_QUEST_MARKERS`]): a u8
/// count, then each its u8 kind ([`QuestMark::kind`]), u8 world, f32 x and z,
/// the NPC's name and a u8 count of quests, each its name, u8 lowest and
/// highest levels, a line and a detail; every text a u8 byte count and UTF-8.
/// Each is the whole set.
pub const OP_QUEST_MARKERS_DATA: u16 = 0x7E04;
/// go-eqoa-server's own too: the quest tracker the HUD draws (each quest
/// under way and what its step asks, counted: "Bat Eye 0/3"), offered and
/// answered as [`OP_QUEST_MARKERS`] is (`agree_quest_progress`).
pub const OP_QUEST_PROGRESS: u16 = 0x7E05;
/// The tracker, from a server the client asked ([`OP_QUEST_PROGRESS`]): a u8
/// count of quests (the journal's, in its order), each its name and a u8
/// count of objectives, each its text, u16 had and u16 needed (needed 0:
/// nothing to count); every text a u8 byte count and UTF-8. Each is the
/// whole tracker.
pub const OP_QUEST_PROGRESS_DATA: u16 = 0x7E06;

/// One quest on the tracker ([`OP_QUEST_PROGRESS_DATA`]): its name and its
/// step's objectives.
#[derive(Clone, Debug, PartialEq)]
pub struct TrackedQuest {
    pub name: String,
    pub goals: Vec<QuestGoal>,
}

/// One objective: its words, and how many the player has of how many are
/// needed (`need` 0: a thing to do, nothing to count).
#[derive(Clone, Debug, PartialEq)]
pub struct QuestGoal {
    pub text: String,
    pub have: u16,
    pub need: u16,
}

impl QuestGoal {
    /// Counted, and all there.
    pub fn done(&self) -> bool {
        self.need > 0 && self.have >= self.need
    }
}

/// [`OP_QUEST_PROGRESS_DATA`]'s body, or None when it is short.
fn read_quest_progress(r: &mut Reader) -> Option<Vec<TrackedQuest>> {
    let text = |r: &mut Reader| {
        let n = r.u8() as usize;
        String::from_utf8_lossy(r.bytes(n)).into_owned()
    };
    let n = r.u8() as usize;
    let mut out = Vec::with_capacity(n);
    for _ in 0..n {
        let name = text(r);
        let g = r.u8() as usize;
        let goals = (0..g)
            .map(|_| {
                let text = text(r);
                QuestGoal { text, have: r.u16(), need: r.u16() }
            })
            .collect();
        out.push(TrackedQuest { name, goals });
    }
    (!r.error).then_some(out)
}

/// One quest mark ([`OP_QUEST_MARKERS_DATA`]).
#[derive(Clone, Debug, PartialEq)]
pub struct QuestMark {
    /// 1 a quest the player may take from this NPC ("!"), 2 a step of its
    /// quests to do at it ("?"), 3 a kill or loot step's creature ("?")
    pub kind: u8,
    /// `gWorldInfo`'s index
    pub world: u8,
    pub x: f32,
    pub z: f32,
    /// the NPC's (or the creature's) name
    pub npc: String,
    pub quests: Vec<MarkQuest>,
}

/// One quest on a mark: its name and levels, a line (the start of the
/// giver's offer, or the step's journal line) and a detail (its rewards, or
/// what the step needs).
#[derive(Clone, Debug, PartialEq)]
pub struct MarkQuest {
    pub name: String,
    pub levels: (u8, u8),
    pub line: String,
    pub detail: String,
}

/// [`OP_QUEST_MARKERS_DATA`]'s body, or None when it is short.
fn read_quest_marks(r: &mut Reader) -> Option<Vec<QuestMark>> {
    let text = |r: &mut Reader| {
        let n = r.u8() as usize;
        String::from_utf8_lossy(r.bytes(n)).into_owned()
    };
    let n = r.u8() as usize;
    let mut out = Vec::with_capacity(n);
    for _ in 0..n {
        let (kind, world, x, z) = (r.u8(), r.u8(), r.f32(), r.f32());
        let npc = text(r);
        let q = r.u8() as usize;
        let quests = (0..q)
            .map(|_| {
                let name = text(r);
                let levels = (r.u8(), r.u8());
                MarkQuest { name, levels, line: text(r), detail: text(r) }
            })
            .collect();
        out.push(QuestMark { kind, world, x, z, npc, quests });
    }
    (!r.error).then_some(out)
}

#[derive(Clone, Debug)]
pub struct Shard {
    pub name: String,
    pub recommended: u8,
    pub endpoint: u64,
    pub language: i8,
}

#[derive(Clone, Debug, Default)]
pub struct Character {
    pub name: String,
    pub server_id: i64,
    pub model: i64,
    pub class: i64,
    pub race: i64,
    pub level: i64,
    pub hair_color: i64,
    pub hair_length: i64,
    pub hair_style: i64,
    pub face: i64,
    /// The 32 raw bytes after the zigzag fields (robe, weapons, armour).
    pub gear: Vec<u8>,
}

/// The head of the MemoryDump: `VIPlayer::Read`, as far as placing the
/// player needs.
#[derive(Clone, Debug, Default)]
pub struct Player {
    /// The dump's `m_p0662C`: the world is not on the disc, so `LoadWorld`
    /// loads `world_path`; clear, it loads the world table's file for `world`.
    pub world_not_on_disc: bool,
    pub world_path: String,
    pub server_id: i64,
    pub name: String,
    pub class: i64,
    pub race: i64,
    pub level: i64,
    pub world: i64,
    pub pos: [f32; 3],
    /// Heading, pitch, roll.
    pub hpr: [f32; 3],
}

/// `VINewChar`, the character-creation form `CreateCharacter` sends as
/// 0x002B (`SendNewChar`). The decomp's pages fill it: race, sex and city
/// (`VIWndCharCreateRace`), class and the stats (`VIWndCharCreateClass`), the
/// look (`VIWndCharCreateMisc`: face, hair style and length 0-3, hair colour
/// 0-7), the name.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct NewChar {
    pub name: String,
    pub race: i32,
    pub class: i32,
    pub sex: i32,
    pub hair_color: i32,
    pub hair_length: i32,
    pub hair_style: i32,
    pub face: i32,
    /// `m_p38`, the city: a human's start (1 and 2), 0 for every other race.
    pub city: i32,
    /// The points spent over the race's and the class's bonuses (the page
    /// sends a stat less both bonuses), 20 at most in all.
    pub stats: [i32; 7],
}

#[derive(Clone, Debug)]
pub enum Event {
    /// 0x0000: the server's version.
    Version(u32),
    ShardList(Vec<Shard>),
    Characters(Vec<Character>),
    /// 0x0012: the name the server picked (`m_wndCharCreate->SetName`).
    RandomName(String),
    /// A creation refused, and the WORDS.TXT line the select page shows:
    /// 0x002E (269), 0x002F (829), 0x0065 (828).
    CreateRefused(i32),
    /// 0x0013: the clock pair (`SetTime`).
    Time(u64, u64),
    /// 0x07D1: the expansion-flag mask.
    Expansion(u32),
    /// 0x007E: the player's pet (`m_petId`); 0x007F: none.
    Pet(Option<u32>),
    /// 0x007B (`Msg007B`): the answer to an assist (0x00CB, 0x007B), the
    /// id to target, -1 for none.
    Assist(u32),
    /// 0x003B: an item into the inventory (`AddInvItem`).
    InvAdd(Box<crate::dump::NetItem>),
    /// 0x003C (`Msg003C`): `n` taken from slot `slot`; `sound` with a merchant.
    InvTook { slot: u8, sound: u8, n: i32 },
    /// 0x003D (`Op003D`): the item at `from` moved to `to`.
    InvMove { from: u8, to: u8 },
    /// 0x003E (`Msg003E`): a charge used (`charge`), else the item gone.
    InvUse { slot: u8, charge: u8 },
    /// 0x003F (`Msg003F`): item `i` worn in equip slot `slot`.
    InvEquip { i: u8, slot: u8 },
    /// 0x0040 (`Msg0040`): equip slot `slot` emptied.
    InvUnequip(u8),
    /// 0x000A (`Msg000A`): item `slot` onto quick slot `pos`.
    QuickSlot { slot: u8, pos: u8 },
    /// 0x0021 (SOE's live client, not the disc: its dispatcher's case at
    /// 0x012CADEC): weapon set `slot`, its name (kept to 15 characters, a
    /// 32-byte buffer) and its main and off hands' items by `VIItem::m_id`
    /// (-1 none).
    GearSet { slot: u8, name: String, main: u32, off: u32 },
    /// 0x0060 (`Msg0060`): the item with inventory id `id` at `hp`.
    ItemHp { id: u32, hp: i32 },
    /// 0x0772 (`Msg0772`): the group's roster, ids and names.
    GroupRoster { ids: Vec<u32>, names: Vec<String> },
    /// 0x0773 (`Msg0773`): an invitation from `id`.
    GroupInvite { id: u32, x: i32, name: String },
    /// 0x0774 (`Msg0774`): the invitation from `id` withdrawn.
    GroupInviteGone(u32),
    /// 0x069A: out of the group, why (`sub`).
    GroupRemoved(u8),
    /// 0x062F: the group disbanded.
    GroupDisbanded,
    /// 0x069C: a group notice: sub 3 alone, 1/2/4/5 about a name.
    GroupNotice { sub: u8, name: String },
    /// 0x00D3 (`Msg00D3`): `name` offers to summon the player (`kind` 0,
    /// word 610) or to resurrect them (else, 611); the box's answer is
    /// 0x00D4.
    SummonPrompt { name: String, kind: u8 },
    /// The server opened the game connection (promotion).
    Promoted,
    /// 0x000D parsed: the player's own entity id and where they stand.
    WorldEntry { self_id: u64, player: Player, dump: Box<crate::dump::MemoryDump> },
    /// 0x00F8: the run speed (0 stops).
    Speed(f32),
    /// 0x0010, the target-info reply (`Msg0010`): the disposition (the con
    /// colour), two more bytes, the title and the guild. Sandstorm's reply
    /// stops before the trailing token, so it is taken as answered whatever
    /// the token, as eqoa-port's `VI_HOST_ONLY` does.
    TargetInfo { disposition: u8, b: u8, c: u8, title: String, guild: String },
    /// 0x0046: a dialog (an NPC's words), raw (`ShowDialog` expands it).
    Dialog { id: u32, text: Vec<u16> },
    /// 0x0034: a dialog with answers (raw; `Msg0034` expands them).
    DialogBox { id: u32, text: Vec<u16>, options: Vec<Vec<u16>> },
    /// 0x00B2 (`Msg00B2`, the respawn/teleport): the world, where, the
    /// heading, pitch and roll, and the token 0x0049 answers with
    /// ([`Session::answer_teleport`], once the move is done).
    Teleport { world: u8, pos: [f32; 3], hpr: [f32; 3], token: u32 },
    /// 0x0020: the player's level and experience.
    LevelExp { level: i64, exp: i64 },
    /// 0x0775: an entity plays an action (`VICSprite::SetAction`).
    Action { id: u32, action: u8 },
    /// 0x07A3: who last sent a tell (`/reply` answers them).
    ReplyName(String),
    /// 0x0045 (`Msg0045`): the profanity filter's state, the server's answer
    /// to the client's own 0x0045 (action 0x0B052000).
    ProfanityFilter(bool),
    /// 0x00D9: the player's +0x2C.
    PlayerP2C(i32),
    /// A line for the chat log, in its colour (RGBA 0..1): 0x0A7A white,
    /// 0x0A7B its own colour, 0x0758 (narrow) white, 0x00CE (an error: "Tell
    /// Failed", an unknown command) red.
    Chat(String, [f32; 4]),
    /// 0x069E, the group's chat: a line for the chat log in `GROUP_CHAT`'s
    /// blue, "<Name> tells the group: <text>" as the servers word it, and the
    /// u32 before it (the sender's id as the server knows it; CLIENT reads
    /// it and lets it go). Its own event, so what reads the group's lines
    /// (the client's group waypoint) never takes another channel's.
    GroupChat { from: u32, text: String },
    /// A state channel's full value (0x00-0x17 entities, 0x42 stats...).
    State { channel: u8, data: Vec<u8> },
    /// The ability list (each carries the list's token, `m_p2BB60`, which
    /// 0x0032 echoes): 0x00B3 one ability (flag: say "learned"), 0x00B4 an
    /// ability by display index onto a slot or off it, 0x00B5 a slot moved,
    /// 0x00B6 a display index moved, 0x00B8 one gone, 0x00CD a lockout.
    AbilityAdd { token: u32, flag: u32, ability: Box<crate::dump::Ability> },
    AbilitySlot { token: u32, index: u8, slot: u8 },
    AbilityMoveSlot { token: u32, from: u8, to: u8 },
    AbilityMoveIndex { token: u32, from: u8, to: u8 },
    AbilityRemove { token: u32, index: i32 },
    AbilityLockout { index: u8, token: u32, lockout: i32 },
    /// 0x00C5 (fades) and 0x00C7 (instant): the weather byte
    /// (`VIClient::SetWeather`).
    Weather { weather: u8, instant: bool },
    /// 0x00FF (`Msg00FF`): every ability (display index below 252) off
    /// the bar; the answer to 0x00FE.
    AbilityClearBar { token: u32 },
    /// 0x00FD (`Msg00FD`): an ability's usable flag (`m_p14`); 0 also
    /// takes it off the bar.
    AbilityUsable { token: u32, id: i32, p14: i32 },
    /// Any other opcode, not handled here yet.
    Opcode { op: u16, data: Vec<u8> },
    /// The connection ended (`VIClient::Disconnect`), with CLIENT's reason
    /// (`m_p4F0E8`, read by `ConnectStatusString`; see `disconnect_word`):
    /// a DRDP 0xFD with its byte mapped as `PumpMessages` maps it, or one of
    /// the dispatcher's disconnect opcodes (0x07A4, "You have logged in
    /// from another location.", is reason 8).
    Disconnected(i32),
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum State {
    Login = 0,
    Shards = 1,
    Shard = 2,
    Characters = 3,
    Selecting = 4,
    World = 5,
}

static DEBUG_SINK: std::sync::OnceLock<fn(&str)> = std::sync::OnceLock::new();

/// Where `EQOA_NET_DEBUG`'s lines go when not to stderr: a page has no
/// stderr, so it sets its console (`?netdebug`).
pub fn set_debug_sink(f: fn(&str)) {
    let _ = DEBUG_SINK.set(f);
}

fn debug_line(s: &str) {
    match DEBUG_SINK.get() {
        Some(f) => f(s),
        None => eprintln!("{s}"),
    }
}

pub struct Session {
    pub drdp: Drdp<Ee>,
    socket: Box<dyn Datagrams>,
    pub conn: Option<ConnId>,
    pub state: State,
    pub self_id: Option<u64>,
    /// 0x07D1's mask (`VIClient+0x50304`), as the server last sent it.
    pub expansion: u32,
    /// The most entity slots and the farthest view distance the server
    /// offered on this connection ([`OP_ENTITY_VIEW`]); none from one that
    /// never does (every retail server, Sandstorm's too).
    pub entity_view_offer: Option<(u32, u32)>,
    /// Whether the server offered others' power on this connection
    /// ([`OP_POWER_BARS`]), what it was last told (none since the offer),
    /// and the set it last sent ([`OP_POWER_BARS_DATA`]: ids and power of
    /// 255).
    pub power_bars_offer: bool,
    power_bars_asked: Option<bool>,
    pub power_bars: Vec<(u32, u8)>,
    /// Whether the server offered quest marks ([`OP_QUEST_MARKERS`]), what it
    /// was last told, and the set it last sent ([`OP_QUEST_MARKERS_DATA`]).
    pub quest_marks_offer: bool,
    quest_marks_asked: Option<bool>,
    pub quest_marks: Vec<QuestMark>,
    /// The same for the quest tracker ([`OP_QUEST_PROGRESS`],
    /// [`OP_QUEST_PROGRESS_DATA`]).
    pub quest_progress_offer: bool,
    quest_progress_asked: Option<bool>,
    pub quest_progress: Vec<TrackedQuest>,
    user: String,
    token: [u8; 32],
    start: Instant,
    base: u32,
    /// Datagrams sent so far: `usocket_sendto_addr` draws newlib's `rand()`
    /// twice for each, so a client keeping animation in step draws as many.
    pub packets_sent: u64,
    /// `EQOA_NET_DEBUG`: every datagram and message on stderr.
    debug: bool,
    /// `m_p382E4`, the chat mode: the last one 0x000E sent (`GroupChanged`
    /// sends it again at every world entry).
    pub chat_mode: u8,
    /// Events handed back to come out of the next `pump` first: what
    /// arrived with the world entry, after it (0x1005, 0x03D5), for the
    /// game to read once it is up.
    pub held: Vec<Event>,
    /// `m_clockA`, `m_clockB`: the server's two clocks (0x0013), synced on
    /// the local timer (`timer`); the time of day is clock B's
    pub clock_a: SyncTime,
    pub clock_b: SyncTime,
    /// While some, every message the game hands the connection, in order,
    /// by its DRDP type (251 guaranteed, under 248 its state channel): a
    /// test's view of what was sent and when, before the transport's own
    /// flow decides when it leaves. None in play.
    pub record: Option<Vec<(u8, Vec<u8>)>>,
}

fn ipv4(a: &SocketAddr) -> [u8; 4] {
    match a {
        SocketAddr::V4(v) => v.ip().octets(),
        SocketAddr::V6(_) => [0; 4],
    }
}

impl Session {
    /// `VIClient::Connect`: a DRDP client on a fresh UDP socket, a connection
    /// to the login endpoint, the hello and 0x0904.
    pub fn connect(server: SocketAddr, user: &str, token: [u8; 32]) -> std::io::Result<Self> {
        let socket = UdpSocket::bind("0.0.0.0:0")?;
        // The local address as the server will see it: route a probe.
        let probe = UdpSocket::bind("0.0.0.0:0")?;
        probe.connect(server)?;
        let local = SocketAddr::new(probe.local_addr()?.ip(), socket.local_addr()?.port());
        socket.set_nonblocking(true)?;
        let udp = Udp { socket, buf: vec![0u8; 65536] };
        Self::connect_with(Box::new(udp), (ipv4(&local), local.port()), (ipv4(&server), server.port()), user, token)
            .ok_or_else(|| std::io::Error::other("connection_open failed"))
    }

    /// `connect` over any datagram carrier: `local` is the address the
    /// server will see the client at (a relay's own, through one).
    pub fn connect_with(socket: Box<dyn Datagrams>, local: ([u8; 4], u16), server: ([u8; 4], u16), user: &str, token: [u8; 32]) -> Option<Self> {
        let base = SystemTime::now().duration_since(UNIX_EPOCH).map(|d| d.as_millis() as u32).unwrap_or(1);
        let mut drdp = Drdp::<Ee>::create_client(Addr { ip: local.0, port: local.1 }, base);
        drdp.set_port_udp(local.1);
        let mut s = Session {
            drdp,
            socket,
            conn: None,
            state: State::Login,
            self_id: None,
            expansion: 0,
            entity_view_offer: None,
            power_bars_offer: false,
            power_bars_asked: None,
            power_bars: Vec::new(),
            quest_marks_offer: false,
            quest_marks_asked: None,
            quest_marks: Vec::new(),
            quest_progress_offer: false,
            quest_progress_asked: None,
            quest_progress: Vec::new(),
            user: user.to_string(),
            token,
            start: Instant::now(),
            base,
            packets_sent: 0,
            debug: std::env::var_os("EQOA_NET_DEBUG").is_some(),
            chat_mode: 0,
            held: Vec::new(),
            clock_a: SyncTime::default(),
            clock_b: SyncTime::default(),
            record: None,
        };
        let dst = address_to_endpoint(server.0, server.1, PORT_LEADER);
        let sys = s.drdp.system_endpoint;
        let c = s.drdp.connection_open(sys, dst)?;
        s.drdp.connection_set_failure(c, 1000, 30000, 60000);
        s.drdp.connection_set_simulated(c, 0, 0, 0);
        s.conn = Some(c);
        s.send_hello();
        let mut w = Writer::op(0x0904);
        w.u8(0).u32(EXPANSION_FLAGS);
        s.send_login(w);
        Some(s)
    }

    fn send_hello(&mut self) {
        let mut w = Writer::op(0x0000);
        w.u32(VERSION);
        self.send(&w.data);
    }

    /// The login tail 0x0904 and 0x0001 share: "EQOA", the name, and the
    /// token in place of the password.
    fn send_login(&mut self, mut w: Writer) {
        let user = self.user.clone();
        w.string(b"EQOA").string(user.as_bytes()).u8(1).bytes(&self.token);
        self.send(&w.data);
    }

    /// `SendRaw`: a guaranteed message on the current connection.
    pub fn send(&mut self, data: &[u8]) {
        if let Some(c) = self.conn {
            if let Some(r) = self.record.as_mut() {
                r.push((drdp::TYPE_GUARANTEED, data.to_vec()));
            }
            self.drdp.connection_send(c, data, drdp::TYPE_GUARANTEED);
        }
    }

    /// A state channel update (0x40 is the player's own state).
    pub fn send_state(&mut self, channel: u8, data: &[u8]) {
        if let Some(c) = self.conn {
            if let Some(r) = self.record.as_mut() {
                r.push((channel, data.to_vec()));
            }
            self.drdp.connection_send(c, data, channel);
        }
    }

    /// `SendOpWStringInt(11, text, target)`: a chat line (wide) and the
    /// target (-1 for none), on the channel in use.
    pub fn say(&mut self, text: &str, target: u32) {
        let mut w = Writer::op(0x000B);
        w.wide(text).u32(target);
        self.send(&w.data);
    }

    /// `SendMsg(op, v, b)`: a u32 and a byte (0x0035 answers a dialog).
    pub fn send_msg(&mut self, op: u16, v: u32, b: u8) {
        let mut w = Writer::op(op);
        w.u32(v).u8(b);
        self.send(&w.data);
    }

    /// `Msg00B2`'s `SendOpSigned(73, token)`: a move of the server's is done
    /// (`Respawn` has stood the player at the arrival). The PS2's frame stops
    /// in `Respawn` while it loads, so this goes in the same packet as its
    /// first 0x40 from there; Sandstorm resumes the state channels on it.
    pub fn answer_teleport(&mut self, token: u32) {
        let mut w = Writer::op(0x0049);
        w.signed(token as i32 as i64);
        self.send(&w.data);
    }

    /// `SendOpcode(op)`: the opcode alone.
    pub fn send_opcode(&mut self, op: u16) {
        let w = Writer::op(op);
        self.send(&w.data);
    }

    /// `SendOpInt(op, v)`.
    pub fn send_op_int(&mut self, op: u16, v: u32) {
        let mut w = Writer::op(op);
        w.u32(v);
        self.send(&w.data);
    }

    /// `SendOpcodeByte(op, v)`.
    /// [`OP_ENTITY_VIEW`] to the server: how many entity slots this client
    /// keeps and how far it draws them (a server that offered them fills that
    /// many from that far).
    pub fn send_entity_view(&mut self, slots: u32, view: u32) {
        let mut w = Writer::op(OP_ENTITY_VIEW);
        w.u32(slots).u32(view);
        self.send(&w.data);
    }

    /// Mana Bars: a server that offered others' power ([`OP_POWER_BARS`])
    /// is told whether to send it (`want`), at once and whenever that
    /// changes; one that made no offer is told nothing. Off, the set it
    /// sent is forgotten.
    pub fn agree_power_bars(&mut self, want: bool) {
        if !want {
            self.power_bars.clear();
        }
        if !self.power_bars_offer || self.power_bars_asked == Some(want) {
            return;
        }
        self.power_bars_asked = Some(want);
        let mut w = Writer::op(OP_POWER_BARS);
        w.u32(want as u32);
        self.send(&w.data);
    }

    /// Quest Markers: a server that offered quest marks
    /// ([`OP_QUEST_MARKERS`]) is told whether to send them (`want`), at once
    /// and whenever that changes; one that made no offer is told nothing.
    /// Off, the set it sent is forgotten.
    pub fn agree_quest_marks(&mut self, want: bool) {
        if !want {
            self.quest_marks.clear();
        }
        if !self.quest_marks_offer || self.quest_marks_asked == Some(want) {
            return;
        }
        self.quest_marks_asked = Some(want);
        let mut w = Writer::op(OP_QUEST_MARKERS);
        w.u32(want as u32);
        self.send(&w.data);
    }

    /// Quest Tracker: a server that offered the tracker
    /// ([`OP_QUEST_PROGRESS`]) is told whether to send it (`want`), as
    /// `agree_quest_marks` tells it of the marks. Off, the tracker it sent is
    /// forgotten.
    pub fn agree_quest_progress(&mut self, want: bool) {
        if !want {
            self.quest_progress.clear();
        }
        if !self.quest_progress_offer || self.quest_progress_asked == Some(want) {
            return;
        }
        self.quest_progress_asked = Some(want);
        let mut w = Writer::op(OP_QUEST_PROGRESS);
        w.u32(want as u32);
        self.send(&w.data);
    }

    /// The power of 255 the server last sent for entity `id`
    /// ([`OP_POWER_BARS_DATA`]), if it sent one.
    pub fn power_of(&self, id: u32) -> Option<u8> {
        self.power_bars.iter().find(|&&(i, _)| i == id).map(|&(_, p)| p)
    }

    pub fn send_op_byte(&mut self, op: u16, v: u8) {
        if op == 0x000E {
            self.chat_mode = v;
        }
        let mut w = Writer::op(op);
        w.u8(v);
        self.send(&w.data);
    }

    /// A sub-op byte and a wide string: `VIClientPart2EF24::Say` (0x0578,
    /// 12: the guild) and `VIClientPart2EFCC::SendText` (0x0C01, 2: a shout).
    pub fn send_op_sub_wide(&mut self, op: u16, sub: u8, text: &str) {
        let mut w = Writer::op(op);
        w.u8(sub).wide(text);
        self.send(&w.data);
    }

    /// `ConnectShard`: the game endpoint, the hello and 0x0001. `hello` is
    /// what follows the hello's opcode when a server's patch makes it more
    /// than retail's DiscVersion (the live S2's: its version, its numbers
    /// and its device block); `None` is retail's.
    pub fn connect_shard(&mut self, endpoint: u64, hello: Option<&[u8]>) {
        self.state = State::Shard;
        if let Some(c) = self.conn.take() {
            self.drdp.connection_close(c);
        }
        let sys = self.drdp.system_endpoint;
        self.conn = self.drdp.connection_open(sys, endpoint);
        match hello {
            Some(h) => {
                let mut w = Writer::op(0x0000);
                w.bytes(h);
                self.send(&w.data);
            }
            None => self.send_hello(),
        }
        let mut w = Writer::op(0x0001);
        w.u8(0).u32(EXPANSION_FLAGS);
        self.send_login(w);
    }

    /// `SendNewChar(43, c)`: the creation form as 0x002B. `m_p18` is 1, as
    /// `VIWndCharCreateBlank::Reset` leaves it; the stats go raw.
    pub fn create_character(&mut self, c: &NewChar) {
        self.send(&new_char_message(c));
    }

    /// `DeleteCharacter`: `SendOpSigned(45, serverId)`.
    pub fn delete_character(&mut self, server_id: i64) {
        let mut w = Writer::op(0x002D);
        w.signed(server_id);
        self.send(&w.data);
    }

    /// `Send0012(race, sex)`: the name page asks the server for a name.
    pub fn random_name(&mut self, race: i32, sex: i32) {
        let mut w = Writer::op(0x0012);
        w.signed(race as i64).signed(sex as i64);
        self.send(&w.data);
    }

    /// `SelectCharacter`: 0x002A with the server id and the face and hair.
    pub fn select_character(&mut self, c: &Character) {
        self.state = State::Selecting;
        let mut w = Writer::op(0x002A);
        for v in [c.server_id, c.face, c.hair_style, c.hair_length, c.hair_color] {
            w.u32(v as u32);
        }
        self.send(&w.data);
    }

    /// The DRDP clock: milliseconds, from the wall clock at connect.
    /// The datagram and message trace `EQOA_NET_DEBUG` turns on.
    pub fn set_debug(&mut self, on: bool) {
        self.debug = on;
    }

    pub fn now(&self) -> u32 {
        self.base.wrapping_add(self.start.elapsed().as_millis() as u32)
    }

    /// The local clock `VIClient` keeps (`m_p07568`), in VITimer's ticks,
    /// 983.04 a second (BUSCLK/16 with a compare of 18,750, plus 2), from
    /// the session's start: what the synced clocks are set and read against.
    pub fn timer(&self) -> i64 {
        (self.start.elapsed().as_micros() as i64).saturating_mul(98_304) / 100_000_000
    }

    /// `VIClient::UpdateTimeOfDay`: the hour, clock B (0x0013's second
    /// reading, blended forward on the local timer) modulo 3,600,000 of its
    /// units as 24 hours, as `VIScene::SetTimeOfDay` clamps it; none until
    /// the server set the clock.
    pub fn time_of_day(&self) -> Option<vi::fpu::F<Ee>> {
        hour_of(&self.clock_b, self.timer())
    }


    /// One frame: the clock, every datagram waiting, the arrived messages
    /// (as events), a flush, and the sends.
    pub fn pump(&mut self) -> Vec<Event> {
        let t = self.now();
        self.drdp.set_time(t);
        while let Some((buf, ip, port)) = self.socket.recv_from() {
            let n = buf.len();
            let mut d = ip.to_vec();
            d.extend(port.to_le_bytes());
            d.extend(&buf);
            let r = self.drdp.process_packet(&d);
            if self.debug {
                let mut b = drdp::buffer::Buffer::reader(&buf[..n.saturating_sub(4)]);
                let (from, to) = (b.u16(), b.u16());
                let mut segs = Vec::new();
                while b.remaining() > 0 && !b.overflow {
                    let h = drdp::read_segment_header(&mut b, 0, 0);
                    let body = b.rest()[..(h.size as usize).min(b.rest().len()).min(16)].to_vec();
                    debug_line(&format!("[net]    body head {body:02x?}"));
                    b.skip(h.size as usize);
                    segs.push(format!("flags {:#x} size {} crc {:#x} src {:#x} dst {:#x}", h.flags, h.size, h.crc, h.src, h.dst));
                }
                debug_line(&format!("[net] <- {n} bytes, drdp ports {from:#x}->{to:#x}: {r}; {segs:?}"));
                let sys = self.drdp.system_endpoint;
                for (&dst, &c) in &self.drdp.eps[sys].connections {
                    let k = &self.drdp.conns[c];
                    debug_line(&format!("[net]    conn {dst:#x}: flags {:#x} flush {} arrival {} rx_seq {} tx_seq {} tx_ack {} inst {:#x}/{:#x} cstat {:?}", k.flags, k.flush, k.arrival_number, k.rx_seqnum, k.tx_seqnum, k.tx_ack, k.instance_local, k.instance_remote, k.cstat));
                }
            }
        }
        let mut events = std::mem::take(&mut self.held);
        while let Some(m) = self.drdp.receive() {
            if self.debug {
                debug_line(&format!("[net] message type {} size {} src {:#x}", m.ty, m.data.len(), m.src));
            }
            self.route(m, &mut events);
        }
        self.flush();
        events
    }

    /// The flush `pump` ends with, and the sends: what was sent goes out now
    /// (Quit's logout, before the client leaves).
    pub fn flush(&mut self) {
        self.drdp.flush();
        for p in std::mem::take(&mut self.drdp.outbox) {
            if self.debug {
                let mut b = drdp::buffer::Buffer::reader(&p.data[..p.data.len().saturating_sub(4)]);
                let (from, to) = (b.u16(), b.u16());
                let mut segs = Vec::new();
                while b.remaining() > 0 && !b.overflow {
                    let h = drdp::read_segment_header(&mut b, 0, 0);
                    let body = b.rest()[..(h.size as usize).min(b.rest().len()).min(12)].to_vec();
                    b.skip(h.size as usize);
                    segs.push(format!("flags {:#x} dst {:#x} body {body:02x?}", h.flags, h.dst));
                }
                debug_line(&format!("[net] -> {} bytes, drdp ports {from:#x}->{to:#x}: {segs:?}", p.data.len()));
            }
            self.socket.send_to(&p.data, p.to.ip, p.to.port);
            self.packets_sent += 1;
        }
    }

    /// `PumpMessages`' routing by DRDP message type.
    fn route(&mut self, m: drdp::Message, ev: &mut Vec<Event>) {
        match m.ty {
            drdp::TYPE_GUARANTEED => self.dispatch(&m, ev),
            drdp::TYPE_BEST_EFFORT => {
                let mut r = Reader::new(&m.data);
                if r.u16() == 0x07B3 && matches!(self.state, State::Login | State::Shards) {
                    ev.push(Event::ShardList(read_shards(&mut r)));
                    self.state = State::Shards;
                }
            }
            drdp::TYPE_SYSTEM => {}
            drdp::TYPE_CONNECT => {
                // The server opened the game connection: the old one goes,
                // the world resets, the new one is ours.
                if let Some(c) = self.conn.take()
                    && Some(c) != m.connection
                    && self.drdp.conns.contains(c)
                {
                    self.drdp.connection_close(c);
                }
                self.conn = m.connection;
                self.self_id = None;
                self.entity_view_offer = None;
                self.power_bars_offer = false;
                self.power_bars_asked = None;
                self.power_bars.clear();
                self.quest_marks_offer = false;
                self.quest_marks_asked = None;
                self.quest_marks.clear();
                self.quest_progress_offer = false;
                self.quest_progress_asked = None;
                self.quest_progress.clear();
                ev.push(Event::Promoted);
            }
            drdp::TYPE_DISCONNECT => {
                if m.connection == self.conn {
                    self.conn = None;
                }
                // m_4F0EC = data[0]; reason 14 -> 16, 1 -> 1, 15 -> 15, else 10
                let reason = match m.data.first().copied().unwrap_or(0) {
                    14 => 16,
                    1 => 1,
                    15 => 15,
                    _ => 10,
                };
                ev.push(Event::Disconnected(reason));
            }
            ty if ty < drdp::TYPE_STATE_CHANNELS => ev.push(Event::State { channel: ty, data: m.data }),
            _ => {}
        }
    }

    /// `ProcessMessage`: one opcode.
    fn dispatch(&mut self, m: &drdp::Message, ev: &mut Vec<Event>) {
        let mut r = Reader::new(&m.data);
        let op = r.u16();
        match op {
            0x0000 => ev.push(Event::Version(r.u32())),
            0x07D1 => {
                self.expansion = r.u32();
                ev.push(Event::Expansion(self.expansion));
            }
            OP_ENTITY_VIEW => {
                let slots = r.u32();
                let view = r.u32();
                if !r.error {
                    self.entity_view_offer = Some((slots, view));
                }
            }
            OP_POWER_BARS => {
                // an offer (again at every world entry): told afresh what
                // the client wants, the server's player being a new one
                r.u32();
                if !r.error {
                    self.power_bars_offer = true;
                    self.power_bars_asked = None;
                }
            }
            OP_POWER_BARS_DATA => {
                let n = r.u8() as usize;
                let set: Vec<(u32, u8)> = (0..n).map(|_| (r.u32(), r.u8())).collect();
                if !r.error {
                    self.power_bars = set;
                }
            }
            OP_QUEST_MARKERS => {
                // an offer (again at every world entry): told afresh
                r.u32();
                if !r.error {
                    self.quest_marks_offer = true;
                    self.quest_marks_asked = None;
                }
            }
            OP_QUEST_MARKERS_DATA => {
                if let Some(set) = read_quest_marks(&mut r) {
                    self.quest_marks = set;
                }
            }
            OP_QUEST_PROGRESS => {
                // an offer (again at every world entry): told afresh
                r.u32();
                if !r.error {
                    self.quest_progress_offer = true;
                    self.quest_progress_asked = None;
                }
            }
            OP_QUEST_PROGRESS_DATA => {
                if let Some(set) = read_quest_progress(&mut r) {
                    self.quest_progress = set;
                }
            }
            0x0013 => {
                // SetTime: each clock moved to the server's reading at the
                // local timer's now
                let (a, b) = (r.u64(), r.u64());
                let now = self.timer();
                self.clock_a.set(a as i64, now);
                self.clock_b.set(b as i64, now);
                ev.push(Event::Time(a, b));
            }
            0x002C if matches!(self.state, State::Shard | State::Characters) => {
                self.state = State::Characters;
                ev.push(Event::Characters(read_characters(&mut r)));
            }
            0x0012 if self.state == State::Characters => ev.push(Event::RandomName(String::from_utf8_lossy(&r.string(24)).into_owned())),
            0x002E if self.state == State::Characters => ev.push(Event::CreateRefused(269)),
            0x002F if self.state == State::Characters => ev.push(Event::CreateRefused(829)),
            0x0065 if self.state == State::Characters => ev.push(Event::CreateRefused(828)),
            0x000D => {
                self.state = State::World;
                let dump = crate::dump::read(&mut r);
                // Every retail dump ends in 39 zero bytes past the last
                // field read (tests/dump_capture.rs).
                if self.debug || r.error || r.pos + 39 != r.data.len() {
                    debug_line(&format!("[net] MemoryDump: {} of {} bytes read{}", r.pos, r.data.len(), if r.error { ", SHORT" } else { "" }));
                }
                let dump_player = dump.player.clone();
                // Msg000D goes on only when the whole dump read.
                if !r.error {
                    let p = &dump.player;
                    let player = Player {
                        world_not_on_disc: dump.world_not_on_disc,
                        world_path: dump.world_path.clone(),
                        server_id: p.id,
                        name: p.name.clone(),
                        class: p.class,
                        race: p.race,
                        level: p.level,
                        world: p.world,
                        pos: p.pos,
                        hpr: p.hpr,
                    };
                    self.self_id = Some(m.src);
                    self.send(&Writer::op(0x0014).data);
                    // GroupChanged(m_p382E4): the chat mode kept
                    self.send_op_byte(0x000E, self.chat_mode);
                    ev.push(Event::WorldEntry { self_id: m.src, player, dump: Box::new(dump) });
                }
                // SendOpByteByte(201, !m_p502D4, 0), (202, !m_p502D8, 0): the
                // options' Faction and Loot Messages OFF bits (m_x93, m_x94),
                // clear by default, as every capture sends them
                let mut w = Writer::op(0x00C9);
                w.u8(0).u8(0);
                self.send(&w.data);
                let mut w = Writer::op(0x00CA);
                w.u8(0).u8(0);
                self.send(&w.data);
                if !r.error {
                    // CLIENT's first frame after the dump writes the 0x40,
                    // and its flush carries it beside the batch above (every
                    // PS2 capture has 0x00CA beside its first 0x40): the
                    // walker as SetLocation leaves it, still, on the floor.
                    // Sandstorm needs it there: a first 0x40 in a later
                    // packet leaves the player out of its world (its own
                    // entity is all it sends, and it never moves it).
                    let p = &dump_player;
                    let phys = vi::walk::WalkPhysics::<Ee> {
                        position: vi::vect::Vect3::of(p.pos[0], p.pos[1], p.pos[2]),
                        angles: vi::vect::Vect3::of(p.hpr[0], 0.0, 0.0),
                        target_heading: vi::fpu::F::new(p.hpr[0]),
                        floor: vi::fpu::F::new(p.pos[1]),
                        flags: vi::walk::flags::ON_FLOOR | vi::walk::flags::TRACK_HEADING,
                        ..Default::default()
                    };
                    let msg = vi::selfstate::encode(p.world as u8, &phys, 0, u32::MAX);
                    self.send_state(0x40, &msg);
                }
            }
            0x00F8 => ev.push(Event::Speed(r.f32())),
            0x00C5 | 0x00C7 => ev.push(Event::Weather { weather: r.u8(), instant: op == 0x00C7 }),
            0x0046 => {
                let id = r.u32();
                let n = r.u32() as usize;
                let raw = r.bytes(n * 2);
                // ReadString(buf, 1024): 511 characters fit.
                let text = if n < 512 { raw.as_chunks::<2>().0.iter().map(|c| u16::from_le_bytes(*c)).collect() } else { Vec::new() };
                ev.push(Event::Dialog { id, text });
            }
            0x0034 => {
                let id = r.u32();
                let wide = |r: &mut Reader, max: usize| -> Vec<u16> {
                    let n = r.u32() as usize;
                    let raw = r.bytes(n * 2);
                    if n < max { raw.as_chunks::<2>().0.iter().map(|c| u16::from_le_bytes(*c)).collect() } else { Vec::new() }
                };
                // ReadString(raw, 256) and each option's ReadString(opt, 128).
                let text = wide(&mut r, 128);
                let n = r.u8();
                let options = (0..n).map(|_| wide(&mut r, 64)).collect();
                ev.push(Event::DialogBox { id, text, options });
            }
            0x00B2 => {
                let world = r.u8();
                let pos = [r.f32(), r.f32(), r.f32()];
                let hpr = [r.f32(), r.f32(), r.f32()];
                let token = r.u32();
                // SendOpSigned(73, token) comes after Respawn is done, so it
                // is the game's to send (`answer_teleport`)
                ev.push(Event::Teleport { world, pos, hpr, token });
            }
            0x0020 => {
                let level = r.signed();
                ev.push(Event::LevelExp { level, exp: r.signed() });
            }
            0x0775 => {
                let id = r.u32();
                ev.push(Event::Action { id, action: r.u8() });
            }
            0x07A3 => ev.push(Event::ReplyName(String::from_utf8_lossy(&r.string(24)).into_owned())),
            0x0045 => ev.push(Event::ProfanityFilter(r.signed() != 0)),
            0x00D9 => ev.push(Event::PlayerP2C(r.u32() as i32)),
            0x007E => ev.push(Event::Pet(Some(r.u32()))),
            0x007B => ev.push(Event::Assist(r.u32())),
            0x003B => ev.push(Event::InvAdd(Box::new(crate::dump::read_net_item(&mut r)))),
            0x003C => {
                let (slot, sound) = (r.u8(), r.u8());
                ev.push(Event::InvTook { slot, sound, n: r.signed() as i32 });
            }
            0x003D => ev.push(Event::InvMove { from: r.u8(), to: r.u8() }),
            0x003E => ev.push(Event::InvUse { slot: r.u8(), charge: r.u8() }),
            0x003F => ev.push(Event::InvEquip { i: r.u8(), slot: r.u8() }),
            0x0040 => ev.push(Event::InvUnequip(r.u8())),
            0x000A => ev.push(Event::QuickSlot { slot: r.u8(), pos: r.u8() }),
            0x0021 => {
                let slot = r.u8();
                let name = r.wide(16);
                ev.push(Event::GearSet { slot, name, main: r.u32(), off: r.u32() });
            }
            0x0060 => {
                let id = r.u32();
                ev.push(Event::ItemHp { id, hp: r.signed() as i32 });
            }
            0x0772 => {
                let _x = r.u32();
                let n = r.u32() as usize;
                // m_group has four members
                let n = n.min(4);
                let ids: Vec<u32> = (0..n).map(|_| r.u32()).collect();
                let names = (0..n).map(|_| String::from_utf8_lossy(&r.string(24)).into_owned()).collect();
                ev.push(Event::GroupRoster { ids, names });
            }
            0x0773 => {
                let (id, x) = (r.u32(), r.u32() as i32);
                let name = String::from_utf8_lossy(&r.string(24)).into_owned();
                ev.push(Event::GroupInvite { id, x, name });
            }
            0x0774 => ev.push(Event::GroupInviteGone(r.u32())),
            0x069A => ev.push(Event::GroupRemoved(r.u8())),
            0x062F => ev.push(Event::GroupDisbanded),
            0x069C => {
                let sub = r.u8();
                let name = if sub == 3 { String::new() } else { String::from_utf8_lossy(&r.string(24)).into_owned() };
                ev.push(Event::GroupNotice { sub, name });
            }
            0x00D3 => {
                // ReadString(name, 64), widened by str8to16 up to a NUL
                let name = r.string(64);
                let n = name.iter().position(|&c| c == 0).unwrap_or(name.len());
                let name = String::from_utf8_lossy(&name[..n]).into_owned();
                ev.push(Event::SummonPrompt { name, kind: r.u8() });
            }
            0x007F => ev.push(Event::Pet(None)),
            0x0010 => {
                let (disposition, b, c) = (r.u8(), r.u8(), r.u8());
                let mut fixed = || {
                    let u: Vec<u16> = (0..64).map(|_| r.u16()).collect();
                    let n = u.iter().position(|&c| c == 0).unwrap_or(64);
                    String::from_utf16_lossy(&u[..n])
                };
                let (title, guild) = (fixed(), fixed());
                ev.push(Event::TargetInfo { disposition, b, c, title, guild });
            }
            0x0A7A => ev.push(Event::Chat(r.wide(1024), [1.0; 4])),
            // ReadString(buf, 2048), AddText in (1, .25, .25)
            0x00CE => ev.push(Event::Chat(r.wide(1024), [1.0, 0.25, 0.25, 1.0])),
            0x0A7B => {
                let text = r.wide(1024);
                let (cr, cg, cb) = (r.u8(), r.u8(), r.u8());
                ev.push(Event::Chat(text, [cr as f32 / 255.0, cg as f32 / 255.0, cb as f32 / 255.0, 1.0]));
            }
            0x069E => {
                let from = r.u32();
                ev.push(Event::GroupChat { from, text: r.wide(1024) });
            }
            0x0758 => ev.push(Event::Chat(String::from_utf8_lossy(&r.string(512)).into_owned(), [1.0; 4])),
            0x00B3 => {
                let (token, flag) = (r.u32(), r.u32());
                let ability = Box::new(crate::dump::read_ability(&mut r));
                ev.push(Event::AbilityAdd { token, flag, ability });
            }
            0x00B4..=0x00B6 => {
                let token = r.u32();
                let (a, b) = (r.u8(), r.u8());
                ev.push(match op {
                    0x00B4 => Event::AbilitySlot { token, index: a, slot: b },
                    0x00B5 => Event::AbilityMoveSlot { token, from: a, to: b },
                    _ => Event::AbilityMoveIndex { token, from: a, to: b },
                });
            }
            0x00FF => ev.push(Event::AbilityClearBar { token: r.u32() }),
            0x00FD => {
                let token = r.u32();
                let (id, p14) = (r.u32() as i32, r.u32() as i32);
                ev.push(Event::AbilityUsable { token, id, p14 });
            }
            0x00B8 => {
                let token = r.u32();
                ev.push(Event::AbilityRemove { token, index: r.signed() as i32 });
            }
            0x00CD => {
                let index = r.u8();
                let token = r.u32();
                ev.push(Event::AbilityLockout { index, token, lockout: r.signed() as i32 });
            }
            // The dispatcher's `Disconnect(reason, 0)` cases: the connection
            // goes, with the reason the notice shows.
            0x0729 | 0x072B | 0x0728 | 0x072A | 0x00EC | 0x07A4 | 0x07A1 | 0x07A2 | 0x07C0 | 0x07C1 | 0x07D2 => {
                let reason = match op {
                    0x0729 => 3,
                    0x072B => 4,
                    0x0728 => 6,
                    0x072A => 7,
                    0x00EC | 0x07A4 => 8,
                    0x07A1 => 13,
                    0x07A2 => 14,
                    0x07C0 => 17,
                    0x07C1 => 18,
                    _ => 19,
                };
                if let Some(c) = self.conn.take()
                    && self.drdp.conns.contains(c)
                {
                    self.drdp.connection_close(c);
                }
                ev.push(Event::Disconnected(reason));
            }
            _ => ev.push(Event::Opcode { op, data: m.data[2.min(m.data.len())..].to_vec() }),
        }
    }
}

/// `VIShardInfo::Read`: entries for the client's language (0) only.
/// `VIClient::ConnectStatusString`: the WORDS.TXT row the disconnect notice
/// shows for a reason (`Event::Disconnected`). Reasons 0 and 10 add a code
/// when there is one, which is left off here.
pub fn disconnect_word(reason: i32) -> Option<i32> {
    Some(match reason {
        0 => 272,
        1 => 666,
        2 => 431,
        3 => 270,
        4 => 593,
        5 => 594,
        6 => 620,
        7 => 621,
        8 => 619,
        9 => 820,
        10 => 667,
        11 => 1017,
        12 => 821,
        13 => 677,
        14 => 716,
        15 => 719,
        16 => 718,
        17 | 18 => 1024,
        19 => 1420,
        20 => 758,
        21 => 822,
        _ => return None,
    })
}

fn read_shards(r: &mut Reader) -> Vec<Shard> {
    let n = r.signed().max(0) as usize;
    let mut v = Vec::new();
    for _ in 0..n {
        let name = r.wide(32);
        let recommended = r.u8();
        let endpoint = r.u64();
        let language = r.u8() as i8;
        if !r.error && language == 0 {
            v.push(Shard { name, recommended, endpoint, language });
        }
    }
    v
}

/// `CharacterList`: at most eight (`m_charCount > 8 ? 8`).
/// 0x002B's bytes (`SendNewChar`): the name as `WriteString` (the form's
/// buffer is 24 bytes), nine zigzags, seven raw ints.
pub fn new_char_message(c: &NewChar) -> Vec<u8> {
    let mut w = Writer::op(0x002B);
    let name: Vec<u8> = c.name.bytes().take(23).collect();
    w.string(&name).signed(1);
    for v in [c.race, c.class, c.sex, c.hair_color, c.hair_length, c.hair_style, c.face, c.city] {
        w.signed(v as i64);
    }
    for v in c.stats {
        w.u32(v as u32);
    }
    w.data
}

fn read_characters(r: &mut Reader) -> Vec<Character> {
    let n = r.signed().clamp(0, 8) as usize;
    (0..n)
        .map(|_| {
            let name = String::from_utf8_lossy(&r.string(24)).into_owned();
            let mut f = [0i64; 9];
            for v in &mut f {
                *v = r.signed();
            }
            let gear = r.bytes(32).to_vec();
            Character {
                name,
                server_id: f[0],
                model: f[1],
                class: f[2],
                race: f[3],
                level: f[4],
                hair_color: f[5],
                hair_length: f[6],
                hair_style: f[7],
                face: f[8],
                gear,
            }
        })
        .collect()
}

/// `VISyncTime` (CLIENT `visynctime.cpp`): a clock synced to the server's.
/// `set` moves it to a new reading without a jump: the difference is
/// blended in over `BLEND_INTERVAL` of local time; `get` reads it at a local
/// time. VIClient keeps two, both set by 0x0013 (`SetTime`); the time of day
/// is clock B's.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct SyncTime {
    /// `m_bReady`
    pub ready: bool,
    /// `m_Blend`: what is left to blend in
    pub blend: i64,
    /// `m_local`: the local clock at the last `set`
    pub local: i64,
    /// `m_remote`: the synced time then
    pub remote: i64,
}

/// `BLEND_INTERVAL`, in the local clock's units.
pub const BLEND_INTERVAL: i64 = 60_000;

impl SyncTime {
    /// `VISyncTime::Set(t, base)`.
    pub fn set(&mut self, t: i64, base: i64) {
        if self.ready {
            let cur = self.get(base);
            self.blend = t.wrapping_sub(cur);
            self.remote = cur;
            self.local = base;
        } else {
            self.ready = true;
            self.local = base;
            self.remote = t;
            self.blend = 0;
        }
    }

    /// `VISyncTime::GetTime(now)` (its `m_bReady` assert left out: callers
    /// ask `ready` first).
    pub fn get(&self, now: i64) -> i64 {
        let dt = now.wrapping_sub(self.local);
        let m = if dt > BLEND_INTERVAL { BLEND_INTERVAL } else { dt };
        let r = self.blend.wrapping_mul(m) / BLEND_INTERVAL;
        self.remote.wrapping_add(dt).wrapping_add(r)
    }
}

/// `UpdateTimeOfDay`'s hour out of `clock` at local time `now`:
/// `(float)(t % 3600000) / 3600000.0f * 24.0f`, then `SetTimeOfDay`'s clamp
/// to 0..23.99999.
pub fn hour_of(clock: &SyncTime, now: i64) -> Option<vi::fpu::F<Ee>> {
    use vi::fpu::F;
    if !clock.ready {
        return None;
    }
    let t = clock.get(now) % 3_600_000;
    let h = F::<Ee>::new(t as f32) / F::new(3_600_000.0) * F::new(24.0);
    Some(vi::fpu::vimin(F::new(23.99999), vi::fpu::vimax(F::ZERO, h)))
}

#[cfg(test)]
mod tests {
    /// `VISyncTime` as visynctime.cpp has it: the first reading taken as it
    /// is and read forward on the local clock; a later one blended in over
    /// 60,000 of it (half the difference halfway, all of it after), never
    /// jumped to. The control: a clock that jumps to the reading.
    #[test]
    fn a_synced_clock_blends_a_new_reading_in() {
        use super::{BLEND_INTERVAL, SyncTime};
        let mut c = SyncTime::default();
        assert!(!c.ready);
        c.set(1_000_000, 500);
        assert_eq!((c.get(500), c.get(10_500)), (1_000_000, 1_010_000));
        // 2,000 ahead at local 20,500, where the clock read 1,020,000
        c.set(1_022_000, 20_500);
        assert_eq!(c.get(20_500), 1_020_000, "no jump");
        assert_eq!(c.get(50_500), 1_020_000 + 30_000 + 1_000, "half of it halfway");
        assert_eq!(c.get(80_500), 1_022_000 + 60_000, "all of it after the interval");
        assert_eq!(c.get(180_500), 1_022_000 + 160_000);
        // C's division: toward zero
        let at = c.get(200_000);
        c.set(at - 7, 200_000);
        assert_eq!(c.get(200_000 + BLEND_INTERVAL / 2), at + BLEND_INTERVAL / 2 - 3);
        // the control
        let jumped = |t: i64, base: i64, now: i64| t + (now - base);
        assert_ne!(jumped(1_022_000, 20_500, 50_500), 1_020_000 + 30_000 + 1_000);
    }

    /// `UpdateTimeOfDay`: clock B modulo 3,600,000 as 24 hours, clamped as
    /// `SetTimeOfDay` clamps; none before the server set the clock.
    #[test]
    fn the_time_of_day_is_clock_b_round_an_hour() {
        use super::{SyncTime, hour_of};
        let mut c = SyncTime::default();
        assert!(hour_of(&c, 0).is_none(), "not set yet");
        c.set(3_600_000 * 500_000 + 1_800_000, 0);
        assert_eq!(hour_of(&c, 0).unwrap().0, 12.0);
        assert_eq!(hour_of(&c, 900_000).unwrap().0, 18.0, "a quarter of the units on");
        assert_eq!(hour_of(&c, 1_800_000).unwrap().0, 0.0, "round again");
        assert_eq!(hour_of(&c, 1_799_999).unwrap().0, 23.99999, "clamped below 24");
    }

    use super::*;

    struct Null;
    impl Datagrams for Null {
        fn send_to(&mut self, _: &[u8], _: [u8; 4], _: u16) {}
        fn recv_from(&mut self) -> Option<(Vec<u8>, [u8; 4], u16)> {
            None
        }
    }

    fn session() -> Session {
        Session::connect_with(Box::new(Null), ([127, 0, 0, 1], 40000), ([127, 0, 0, 1], 10070), "test", [0; 32]).unwrap()
    }

    fn deliver(s: &mut Session, ty: u8, data: Vec<u8>) -> Vec<Event> {
        let mut ev = Vec::new();
        s.route(drdp::Message { ty, data, connection: s.conn, ..Default::default() }, &mut ev);
        ev
    }

    // 0x00B2 is the game's to answer, after its Respawn (`Play::msg_00b2`):
    // the session hands the move on with its token and sends nothing (it
    // answered at once until 2026-10-25, which with a world still loading
    // is not a PS2's order); `answer_teleport` is 0x0049 and the token,
    // zigzag. Control: the record does see a send.
    #[test]
    fn a_move_is_handed_on_unanswered() {
        let mut s = session();
        s.record = Some(Vec::new());
        let mut w = Writer::op(0x00B2);
        w.u8(2);
        for v in [1.0f32, 2.0, 3.0, 0.5, 0.0, 0.0] {
            w.u32(v.to_bits());
        }
        w.u32(7);
        let ev = deliver(&mut s, drdp::TYPE_GUARANTEED, w.data);
        assert!(matches!(ev[..], [Event::Teleport { world: 2, token: 7, .. }]), "{ev:?}");
        assert_eq!(s.record.as_deref(), Some(&[][..]), "nothing sent on receipt");
        s.answer_teleport(7);
        assert_eq!(s.record.unwrap(), [(drdp::TYPE_GUARANTEED, vec![0x49, 0x00, 14])]);
    }

    // 0x07A4, what go-eqoa-server sends a session whose character logged in
    // from another window: CLIENT's Disconnect(8), word 619, and the
    // connection goes. Control: an opcode that is not a disconnect keeps it.
    #[test]
    fn logged_in_elsewhere_disconnects() {
        let mut s = session();
        let ev = deliver(&mut s, drdp::TYPE_GUARANTEED, vec![0xA4, 0x07]);
        assert!(matches!(ev[..], [Event::Disconnected(8)]), "{ev:?}");
        assert!(s.conn.is_none());
        assert_eq!(disconnect_word(8), Some(619));

        let mut s = session();
        let ev = deliver(&mut s, drdp::TYPE_GUARANTEED, vec![0xA5, 0x07]);
        assert!(!ev.iter().any(|e| matches!(e, Event::Disconnected(_))), "{ev:?}");
        assert!(s.conn.is_some());
    }

    // 0x00CE, the server's error line, is a chat line in red, as CLIENT's
    // dispatcher adds it; it was dropped, so a refused tell or an unknown
    // command said nothing. The control: 0x0A7A is white.
    #[test]
    fn error_text_is_a_red_line() {
        let wide = |op: u16, t: &str| {
            let mut v = op.to_le_bytes().to_vec();
            v.extend((t.encode_utf16().count() as u32).to_le_bytes());
            v.extend(t.encode_utf16().flat_map(|c| c.to_le_bytes()));
            v
        };
        let mut s = session();
        let ev = deliver(&mut s, drdp::TYPE_GUARANTEED, wide(0x00CE, "Tell Failed"));
        assert!(matches!(&ev[..], [Event::Chat(t, c)] if t == "Tell Failed" && *c == [1.0, 0.25, 0.25, 1.0]), "{ev:?}");
        let ev = deliver(&mut s, drdp::TYPE_GUARANTEED, wide(0x0A7A, "hi"));
        assert!(matches!(&ev[..], [Event::Chat(t, c)] if t == "hi" && *c == [1.0; 4]), "{ev:?}");
    }

    // 0x069E, the group's chat (a u32, then the line), is its own event: what
    // reads the group's lines never takes another channel's. The control: the
    // same words on 0x0A7A are a plain chat line.
    #[test]
    fn a_group_line_is_its_own_event() {
        let text = "Dairine tells the group: Waypoint in Tunaria at x 1, z 2";
        let mut v = 0x069Eu16.to_le_bytes().to_vec();
        v.extend(0x0001_4F51u32.to_le_bytes());
        v.extend((text.encode_utf16().count() as u32).to_le_bytes());
        v.extend(text.encode_utf16().flat_map(|c| c.to_le_bytes()));
        let mut s = session();
        let ev = deliver(&mut s, drdp::TYPE_GUARANTEED, v);
        assert!(matches!(&ev[..], [Event::GroupChat { from: 0x0001_4F51, text: t }] if t == text), "{ev:?}");
        let mut w = 0x0A7Au16.to_le_bytes().to_vec();
        w.extend((text.encode_utf16().count() as u32).to_le_bytes());
        w.extend(text.encode_utf16().flat_map(|c| c.to_le_bytes()));
        let ev = deliver(&mut s, drdp::TYPE_GUARANTEED, w);
        assert!(matches!(&ev[..], [Event::Chat(t, _)] if t == text), "the control: {ev:?}");
    }

    /// 0x0013 (`SetTime`) sets both clocks at the local timer's now, and the
    /// time of day is clock B's; none before it came (the control).
    #[test]
    fn set_time_sets_the_clocks() {
        let mut s = session();
        assert!(s.time_of_day().is_none());
        let (a, b) = (1_792_732_374_253u64, 3_600_000u64 * 498_000 + 900_000);
        let mut m = 0x0013u16.to_le_bytes().to_vec();
        m.extend(a.to_le_bytes());
        m.extend(b.to_le_bytes());
        let ev = deliver(&mut s, drdp::TYPE_GUARANTEED, m);
        assert!(matches!(&ev[..], [Event::Time(x, y)] if (*x, *y) == (a, b)), "{ev:?}");
        assert!(s.clock_a.ready && s.clock_b.ready);
        assert_eq!(s.clock_b.remote, b as i64);
        let h = s.time_of_day().unwrap().0;
        assert!((6.0..6.01).contains(&h), "a quarter round clock B's hour: {h}");
    }

    // 0x0045's zigzag: 2 is on, 0 off (Msg0045's ReadSigned). The control:
    // the byte read raw would make 2 "on" and 1 "on" too.
    #[test]
    fn profanity_filter_state() {
        for (byte, on) in [(2u8, true), (0, false), (1, true)] {
            let mut s = session();
            let ev = deliver(&mut s, drdp::TYPE_GUARANTEED, vec![0x45, 0x00, byte]);
            assert!(matches!(ev[..], [Event::ProfanityFilter(o)] if o == on), "{byte}: {ev:?}");
        }
        let mut s = session();
        let ev = deliver(&mut s, drdp::TYPE_GUARANTEED, vec![0x45, 0x00, 0x00]);
        assert!(!matches!(ev[..], [Event::ProfanityFilter(true)]), "zero is off");
    }

    // 0x00D3, the summons and resurrection offer: the caster's name
    // (ReadString(name, 64)) and the kind. It went by as an unhandled
    // opcode, so a resurrection on eqoa.live put up no box. The control:
    // a name of 64 bytes or more is read as empty, the kind still after it.
    #[test]
    fn summon_prompt_reads_the_name_and_kind() {
        let msg = |name: &[u8], kind: u8| {
            let mut w = Writer::op(0x00D3);
            w.string(name).u8(kind);
            w.data
        };
        let mut s = session();
        let ev = deliver(&mut s, drdp::TYPE_GUARANTEED, msg(b"Healer", 1));
        assert!(matches!(&ev[..], [Event::SummonPrompt { name, kind: 1 }] if name == "Healer"), "{ev:?}");
        let ev = deliver(&mut s, drdp::TYPE_GUARANTEED, msg(b"Mage\0junk", 0));
        assert!(matches!(&ev[..], [Event::SummonPrompt { name, kind: 0 }] if name == "Mage"), "{ev:?}");
        let ev = deliver(&mut s, drdp::TYPE_GUARANTEED, msg(&[b'x'; 64], 1));
        assert!(matches!(&ev[..], [Event::SummonPrompt { name, kind: 1 }] if name.is_empty()), "{ev:?}");
    }

    // A DRDP 0xFD carries the library's byte, mapped as PumpMessages maps it.
    #[test]
    fn drdp_disconnect_maps_its_byte() {
        for (byte, reason) in [(14, 16), (1, 1), (15, 15), (0, 10), (7, 10)] {
            let mut s = session();
            let ev = deliver(&mut s, drdp::TYPE_DISCONNECT, vec![byte]);
            assert!(matches!(ev[..], [Event::Disconnected(r)] if r == reason), "{byte}: {ev:?}");
        }
    }

    /// Every datagram sent, kept.
    struct Rec(std::rc::Rc<std::cell::RefCell<Vec<Vec<u8>>>>);
    impl Datagrams for Rec {
        fn send_to(&mut self, d: &[u8], _: [u8; 4], _: u16) {
            self.0.borrow_mut().push(d.to_vec());
        }
        fn recv_from(&mut self) -> Option<(Vec<u8>, [u8; 4], u16)> {
            None
        }
    }

    /// A whole MemoryDump with nothing in its lists: the player at `pos`.
    fn empty_dump(pos: [f32; 3], heading: f32) -> Vec<u8> {
        let mut w = Writer::op(0x000D);
        w.u8(0).string(b"data\\tunaria.esf");
        w.signed(7).string(b"Tester");
        for v in [12, 8, 49, 0, 0] {
            w.signed(v);
        }
        w.u8(255);
        for v in [0, 0, 0, 0, 0] {
            w.signed(v);
        }
        for v in pos.into_iter().chain([heading, 0.0, 0.0]) {
            w.u32(v.to_bits());
        }
        w.signed(0).u32(0).u32(0).u32(0); // text sets, the two tokens, quests
        w.signed(0).u32(0); // inventory
        w.signed(0).u32(0).signed(0).signed(0).u8(0); // bank, lots, bids
        w.u8(0).signed(0).u32(8.0f32.to_bits()); // no corpse, no abilities, the speed
        for _ in 0..25 {
            w.u32(0); // the crafting slots
        }
        w.u32(0);
        for _ in 0..4 {
            w.signed(0);
        }
        w.u32(8.0f32.to_bits());
        for _ in 0..36 {
            w.signed(0); // Combat Mastery
        }
        w.bytes(&[0; 39]);
        w.data
    }

    /// The messages in one of the client's datagrams: (type, payload), a
    /// guaranteed one's payload starting with its opcode, a state's RLE undone.
    fn messages(d: &[u8]) -> Vec<(u8, Vec<u8>)> {
        use drdp::buffer::{Buffer, rle_decode};
        let mut out = Vec::new();
        let mut b = Buffer::reader(&d[..d.len() - 4]);
        b.u16();
        b.u16();
        while b.remaining() > 0 && !b.overflow {
            let h = drdp::read_segment_header(&mut b, 0, 0);
            let mut s = Buffer::reader(&b.rest()[..(h.size as usize).min(b.rest().len())]);
            b.skip(h.size as usize);
            if h.flags & 0x30000 != 0 || s.max == 0 {
                continue;
            }
            let f = s.u8();
            if f & 0x40 != 0 {
                s.u32();
            }
            s.u16();
            if f & 1 != 0 {
                s.u16();
                if f & 4 != 0 {
                    s.packed();
                }
            }
            if f & 2 != 0 {
                s.u16();
                if f & 8 != 0 {
                    s.packed();
                }
            }
            if f & 0x10 != 0 {
                while s.u8() != 248 && !s.overflow {
                    s.u16();
                }
            }
            while s.remaining() > 0 && !s.overflow {
                let ty = s.u8();
                let size = s.size() as usize;
                if ty.wrapping_add(7) < 3 {
                    s.u16();
                    out.push((ty, s.rest()[..size].to_vec()));
                    s.skip(size);
                } else if ty == 252 {
                    s.skip(size);
                } else if ty < 248 {
                    s.u16();
                    s.u8();
                    let mut v = vec![0; size];
                    rle_decode(&mut s, &mut v);
                    out.push((ty, v));
                }
            }
        }
        out
    }

    // go-eqoa-server's 0x7E00 offer is kept for the connection, and gone
    // with it (a new one starts with none); a short one is not taken. The
    // answer is the opcode and two u32s. The control: no offer before one came.
    #[test]
    fn entity_view_offer_and_answer() {
        let sent = std::rc::Rc::new(std::cell::RefCell::new(Vec::new()));
        let mut s = Session::connect_with(Box::new(Rec(sent.clone())), ([127, 0, 0, 1], 40000), ([127, 0, 0, 1], 10070), "test", [0; 32]).unwrap();
        assert_eq!(s.entity_view_offer, None, "no offer before one came");
        deliver(&mut s, drdp::TYPE_GUARANTEED, vec![0x00, 0x7E, 204, 0, 0, 0]);
        assert_eq!(s.entity_view_offer, None, "a short offer");
        let ev = deliver(&mut s, drdp::TYPE_GUARANTEED, vec![0x00, 0x7E, 204, 0, 0, 0, 0xE8, 0x03, 0, 0]);
        assert!(ev.is_empty(), "{ev:?}");
        assert_eq!(s.entity_view_offer, Some((204, 1000)));

        s.flush();
        sent.borrow_mut().clear();
        s.send_entity_view(128, 600);
        s.flush();
        let m: Vec<_> = sent.borrow().iter().flat_map(|d| messages(d)).collect();
        assert_eq!(m, [(251, vec![0x00, 0x7E, 128, 0, 0, 0, 0x58, 0x02, 0, 0])]);

        deliver(&mut s, drdp::TYPE_CONNECT, Vec::new());
        assert_eq!(s.entity_view_offer, None, "a new connection, no offer yet");
    }

    // go-eqoa-server's 0x7E01 offer: nothing is asked before it, the answer
    // goes once for each change of mind (1 to have the power, 0 to stop), and
    // a new offer (a world entry) is answered afresh. Its 0x7E02 set replaces
    // the last whole; a short one is not taken; off, or on a new connection,
    // there is none.
    #[test]
    fn power_bars_offer_answer_and_set() {
        let sent = std::rc::Rc::new(std::cell::RefCell::new(Vec::new()));
        let mut s = Session::connect_with(Box::new(Rec(sent.clone())), ([127, 0, 0, 1], 40000), ([127, 0, 0, 1], 10070), "test", [0; 32]).unwrap();
        let answers = |s: &mut Session| -> Vec<(u8, Vec<u8>)> {
            s.flush();
            let m = sent.borrow().iter().flat_map(|d| messages(d)).filter(|(_, d)| d.starts_with(&[0x01, 0x7E])).collect();
            sent.borrow_mut().clear();
            m
        };
        s.agree_power_bars(true);
        assert_eq!(answers(&mut s), [], "no offer, nothing asked");
        let ev = deliver(&mut s, drdp::TYPE_GUARANTEED, vec![0x01, 0x7E, 1, 0, 0, 0]);
        assert!(ev.is_empty(), "{ev:?}");
        assert!(s.power_bars_offer);
        s.agree_power_bars(true);
        s.agree_power_bars(true);
        assert_eq!(answers(&mut s), [(251, vec![0x01, 0x7E, 1, 0, 0, 0])], "asked once");
        s.agree_power_bars(false);
        assert_eq!(answers(&mut s), [(251, vec![0x01, 0x7E, 0, 0, 0, 0])], "and stopped");
        s.agree_power_bars(true);
        assert_eq!(answers(&mut s), [(251, vec![0x01, 0x7E, 1, 0, 0, 0])]);

        let ev = deliver(&mut s, drdp::TYPE_GUARANTEED, vec![0x02, 0x7E, 2, 0xA1, 0x86, 1, 0, 63, 0xEA, 3, 0, 0x7F, 255]);
        assert!(ev.is_empty(), "{ev:?}");
        assert_eq!((s.power_of(100001), s.power_of(0x7F0003EA), s.power_of(100002)), (Some(63), Some(255), None));
        deliver(&mut s, drdp::TYPE_GUARANTEED, vec![0x02, 0x7E, 2, 0xA1, 0x86, 1, 0, 9]);
        assert_eq!(s.power_of(100001), Some(63), "a short set is not taken");
        deliver(&mut s, drdp::TYPE_GUARANTEED, vec![0x02, 0x7E, 1, 0xA2, 0x86, 1, 0, 9]);
        assert_eq!((s.power_of(100001), s.power_of(100002)), (None, Some(9)), "each set is the whole set");

        deliver(&mut s, drdp::TYPE_GUARANTEED, vec![0x01, 0x7E, 1, 0, 0, 0]);
        s.agree_power_bars(true);
        assert_eq!(answers(&mut s), [(251, vec![0x01, 0x7E, 1, 0, 0, 0])], "a new offer, asked again");
        s.agree_power_bars(false);
        assert_eq!(s.power_of(100002), None, "off: the set forgotten");

        deliver(&mut s, drdp::TYPE_CONNECT, Vec::new());
        assert!(!s.power_bars_offer && s.power_bars.is_empty(), "a new connection, no offer yet");
        s.agree_power_bars(true);
        assert_eq!(answers(&mut s), [], "and nothing asked");
    }

    // Quest marks (go-eqoa-server's 0x7E03/0x7E04): nothing asked without an
    // offer; an offer answered once for each change of mind; a set read whole
    // (a giver's "!" with its quest's levels, offer and rewards), a short one
    // not taken, off forgotten.
    #[test]
    fn quest_marks_offer_answer_and_set() {
        let sent = std::rc::Rc::new(std::cell::RefCell::new(Vec::new()));
        let mut s = Session::connect_with(Box::new(Rec(sent.clone())), ([127, 0, 0, 1], 40000), ([127, 0, 0, 1], 10070), "test", [0; 32]).unwrap();
        let answers = |s: &mut Session| -> Vec<(u8, Vec<u8>)> {
            s.flush();
            let m = sent.borrow().iter().flat_map(|d| messages(d)).filter(|(_, d)| d.starts_with(&[0x03, 0x7E])).collect();
            sent.borrow_mut().clear();
            m
        };
        s.agree_quest_marks(true);
        assert_eq!(answers(&mut s), [], "no offer, nothing asked");
        deliver(&mut s, drdp::TYPE_GUARANTEED, vec![0x03, 0x7E, 1, 0, 0, 0]);
        s.agree_quest_marks(true);
        s.agree_quest_marks(true);
        assert_eq!(answers(&mut s), [(251, vec![0x03, 0x7E, 1, 0, 0, 0])], "asked once");

        let text = |b: &mut Vec<u8>, t: &str| {
            b.push(t.len() as u8);
            b.extend_from_slice(t.as_bytes());
        };
        let mut m = vec![0x04, 0x7E, 1, 1, 0];
        m.extend_from_slice(&25403.5f32.to_le_bytes());
        m.extend_from_slice(&15725.5f32.to_le_bytes());
        text(&mut m, "Malsis");
        m.push(1);
        text(&mut m, "The pincers");
        m.extend_from_slice(&[1, 10]);
        text(&mut m, "You there.");
        text(&mut m, "430 XP, Smoldering Aura");
        deliver(&mut s, drdp::TYPE_GUARANTEED, m.clone());
        assert_eq!(
            s.quest_marks,
            [QuestMark {
                kind: 1,
                world: 0,
                x: 25403.5,
                z: 15725.5,
                npc: "Malsis".into(),
                quests: vec![MarkQuest { name: "The pincers".into(), levels: (1, 10), line: "You there.".into(), detail: "430 XP, Smoldering Aura".into() }],
            }]
        );
        deliver(&mut s, drdp::TYPE_GUARANTEED, m[..m.len() - 4].to_vec());
        assert_eq!(s.quest_marks.len(), 1, "a short set is not taken");
        s.agree_quest_marks(false);
        assert_eq!(answers(&mut s), [(251, vec![0x03, 0x7E, 0, 0, 0, 0])], "and stopped");
        assert!(s.quest_marks.is_empty(), "off: the set forgotten");
    }

    /// The quest tracker's offer is answered once, its body read whole (the
    /// counts, a goal done only counted and all there), a short one not
    /// taken (the control), and off it is stopped and forgotten.
    #[test]
    fn quest_progress_offer_answer_and_set() {
        let sent = std::rc::Rc::new(std::cell::RefCell::new(Vec::new()));
        let mut s = Session::connect_with(Box::new(Rec(sent.clone())), ([127, 0, 0, 1], 40000), ([127, 0, 0, 1], 10070), "test", [0; 32]).unwrap();
        let answers = |s: &mut Session| -> Vec<(u8, Vec<u8>)> {
            s.flush();
            let m = sent.borrow().iter().flat_map(|d| messages(d)).filter(|(_, d)| d.starts_with(&[0x05, 0x7E])).collect();
            sent.borrow_mut().clear();
            m
        };
        s.agree_quest_progress(true);
        assert_eq!(answers(&mut s), [], "no offer, nothing asked");
        deliver(&mut s, drdp::TYPE_GUARANTEED, vec![0x05, 0x7E, 1, 0, 0, 0]);
        s.agree_quest_progress(true);
        s.agree_quest_progress(true);
        assert_eq!(answers(&mut s), [(251, vec![0x05, 0x7E, 1, 0, 0, 0])], "asked once");
        let text = |b: &mut Vec<u8>, t: &str| {
            b.push(t.len() as u8);
            b.extend_from_slice(t.as_bytes());
        };
        let mut m = vec![0x06, 0x7E, 1];
        text(&mut m, "Bat eyes");
        m.push(2);
        text(&mut m, "Bat Eye");
        m.extend_from_slice(&[1, 0, 3, 0]);
        text(&mut m, "Give to Kellina");
        m.extend_from_slice(&[0, 0, 0, 0]);
        deliver(&mut s, drdp::TYPE_GUARANTEED, m.clone());
        let want = [TrackedQuest { name: "Bat eyes".into(), goals: vec![QuestGoal { text: "Bat Eye".into(), have: 1, need: 3 }, QuestGoal { text: "Give to Kellina".into(), have: 0, need: 0 }] }];
        assert_eq!(s.quest_progress, want);
        assert!(!s.quest_progress[0].goals[0].done() && !s.quest_progress[0].goals[1].done(), "1 of 3, and nothing counted");
        assert!(QuestGoal { text: String::new(), have: 3, need: 3 }.done());
        deliver(&mut s, drdp::TYPE_GUARANTEED, m[..m.len() - 2].to_vec());
        assert_eq!(s.quest_progress, want, "a short one is not taken");
        s.agree_quest_progress(false);
        assert_eq!(answers(&mut s), [(251, vec![0x05, 0x7E, 0, 0, 0, 0])], "and stopped");
        assert!(s.quest_progress.is_empty(), "off: forgotten");
    }

    // Msg000D's world entry is ONE datagram, as every PS2 capture's is: 0x0014,
    // GroupChanged's 0x000E with the chat mode kept, 0x00C9 and 0x00CA, and the
    // first frame's 0x40 at the dump's place. Sandstorm needs the 0x40 there:
    // sent in a later packet, the player is left out of its world (no entity
    // but its own, and it never moves). The control: a dump cut short sends
    // 0x00C9 and 0x00CA alone (Msg000D's Msg0777 path), and no 0x40.
    #[test]
    fn world_entry_carries_the_first_self_state() {
        let pos = [25296.398, 54.125977, 15785.668];
        let sent = std::rc::Rc::new(std::cell::RefCell::new(Vec::new()));
        let mut s = Session::connect_with(Box::new(Rec(sent.clone())), ([127, 0, 0, 1], 40000), ([127, 0, 0, 1], 10070), "test", [0; 32]).unwrap();
        s.flush();
        s.send_op_byte(0x000E, 3);
        s.flush();
        sent.borrow_mut().clear();
        let dump = empty_dump(pos, 3.5588355);
        let mut ev = Vec::new();
        s.route(drdp::Message { ty: drdp::TYPE_GUARANTEED, data: dump.clone(), connection: s.conn, src: 0x1d3d_8fa9, ..Default::default() }, &mut ev);
        assert!(matches!(ev[..], [Event::WorldEntry { self_id: 0x1d3d_8fa9, .. }]), "{ev:?}");
        s.flush();
        let sent_now = sent.borrow().clone();
        assert_eq!(sent_now.len(), 1, "the world entry is one datagram");
        let m = messages(&sent_now[0]);
        let kinds: Vec<(u8, u16)> = m.iter().map(|(t, d)| (*t, if *t == 251 { u16::from_le_bytes([d[0], d[1]]) } else { 0 })).collect();
        assert_eq!(kinds, [(251, 0x0014), (251, 0x000E), (251, 0x00C9), (251, 0x00CA), (0x40, 0)], "{m:02x?}");
        assert_eq!(m[1].1[2..], [3], "the chat mode kept");
        assert_eq!(m[2].1[2..], [0, 0]);
        assert_eq!(m[3].1[2..], [0, 0]);
        let phys = vi::walk::WalkPhysics::<Ee> {
            position: vi::vect::Vect3::of(pos[0], pos[1], pos[2]),
            angles: vi::vect::Vect3::of(3.5588355, 0.0, 0.0),
            target_heading: vi::fpu::F::new(3.5588355),
            floor: vi::fpu::F::new(pos[1]),
            flags: vi::walk::flags::ON_FLOOR | vi::walk::flags::TRACK_HEADING,
            ..Default::default()
        };
        assert_eq!(m[4].1, vi::selfstate::encode(0, &phys, 0, u32::MAX), "the 0x40 at the dump's place");

        // A dump cut short: no world entry, no 0x40.
        let sent = std::rc::Rc::new(std::cell::RefCell::new(Vec::new()));
        let mut s = Session::connect_with(Box::new(Rec(sent.clone())), ([127, 0, 0, 1], 40000), ([127, 0, 0, 1], 10070), "test", [0; 32]).unwrap();
        s.flush();
        sent.borrow_mut().clear();
        let mut ev = Vec::new();
        s.route(drdp::Message { ty: drdp::TYPE_GUARANTEED, data: dump[..dump.len() - 80].to_vec(), connection: s.conn, src: 0x1d3d_8fa9, ..Default::default() }, &mut ev);
        assert!(ev.is_empty(), "{ev:?}");
        s.flush();
        let m = messages(&sent.borrow()[0]);
        let kinds: Vec<(u8, u16)> = m.iter().map(|(t, d)| (*t, if *t == 251 { u16::from_le_bytes([d[0], d[1]]) } else { 0 })).collect();
        assert_eq!(kinds, [(251, 0x00C9), (251, 0x00CA)], "{m:02x?}");
    }
}
