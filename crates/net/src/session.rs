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

use crate::addr::Nat64;
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

/// A non-blocking UDP socket: IPv4, or on a network with IPv6 alone an IPv6
/// one through the network's gateway to IPv4 (`addr::Nat64`).
pub struct Udp {
    socket: UdpSocket,
    buf: Vec<u8>,
    nat64: Option<Nat64>,
}

impl Datagrams for Udp {
    fn send_to(&mut self, data: &[u8], ip: [u8; 4], port: u16) {
        let to = match &self.nat64 {
            Some(n) => n.to_v6(ip, port),
            None => SocketAddr::from((ip, port)),
        };
        let _ = self.socket.send_to(data, to);
    }

    fn recv_from(&mut self) -> Option<(Vec<u8>, [u8; 4], u16)> {
        loop {
            let (n, p) = self.socket.recv_from(&mut self.buf).ok()?;
            let ip = match &self.nat64 {
                // one not from under the gateway's prefix is no server's
                Some(g) => match g.to_v4(&p) {
                    Some(ip) => ip,
                    None => continue,
                },
                None => ipv4(&p),
            };
            return Some((self.buf[..n].to_vec(), ip, p.port()));
        }
    }
}

/// `VIClient`'s protocol version (0x0000 carries it both ways).
pub const VERSION: u32 = 18;
/// The expansion flags a retail Frontiers disc sends.
pub const EXPANSION_FLAGS: u32 = 3;
/// go-eqoa-server's own, no retail message: what a client keeps in view, how
/// many entity slots (`entity`) and how far, two u32s after the opcode, both
/// ways. The server offers the most it fills and the farthest it looks once
/// the player is in the world; the client answers with its NPC/PC Display and
/// NPC/PC View Distance, and again whenever they change. CLIENT has no case
/// for it (its dispatcher has no default), so a PS2 ignores the offer, and
/// nothing is sent to a server that made none.
pub const OP_ENTITY_VIEW: u16 = 0x7E00;
/// go-eqoa-server's own, no retail message either: the GM's trigger names, a
/// u32 after the opcode, both ways. The server offers them (1) to a GM's
/// client beside the GM channel's 0x0700 35; this client answers 1 at once
/// and is then sent the names (0x7E08, `admin::read_triggers`) whenever they
/// change. A PS2 ignores the offer, as it ignores [`OP_ENTITY_VIEW`]'s.
pub const OP_GM_TRIGGERS: u16 = 0x7E07;
/// go-eqoa-server's own, no retail message either: the live game's duel
/// messages (0x1501 up, 0x1502 down, the client's `duel` part and its
/// window), a u32 after the opcode, both ways. The server offers them (1)
/// once the player is in the world; this client has the window and answers 1
/// at once, and is then sent 0x1502 where a client without it gets boxes. A
/// PS2 ignores the offer, as it ignores [`OP_ENTITY_VIEW`]'s.
pub const OP_DUEL_WINDOW: u16 = 0x7E09;
/// go-eqoa-server's own, no retail message either: SOE's live game's look, a
/// u32 after the opcode, both ways. The server offers it (1) after the
/// session's first character list; this client answers 1 at once: it reads
/// the live CLIENT's entity channel ([`crate::entity::LIVE_STATE_SIZE`]
/// bytes, a colour per held item, armour slot and robe) and its 0x002C
/// record ([`LIVE_GEAR_SIZE`] raw bytes). A PS2 ignores the offer; Sandstorm
/// never makes one, so its channels and lists stay the disc's.
pub const OP_LIVE_LOOK: u16 = 0x7E0C;
/// The server's answer to [`OP_LIVE_LOOK`], a u32: the look in force from it
/// on (1 the live game's, 0 the disc's). Every character list after it is in
/// that look (lists are guaranteed messages, in order); a client still
/// choosing is sent its list again.
pub const OP_LIVE_LOOK_IN_FORCE: u16 = 0x7E0D;
/// The raw part of a 0x002C record: the disc's, and the live game's
/// (`CharacterList`, Corsten's 0x012CC018: no robe tint, six ArmorGroup
/// variants where the disc's has tints, then the held items', armour slots'
/// and robe's colours).
pub const DISC_GEAR_SIZE: usize = 32;
pub const LIVE_GEAR_SIZE: usize = 71;

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
    /// The raw bytes after the zigzag fields (robe, weapons, armour): the
    /// disc's [`DISC_GEAR_SIZE`], or the live game's [`LIVE_GEAR_SIZE`] when
    /// its look is in force.
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
    /// 0x0A7B its own colour, 0x069E blue, 0x0758 (narrow) white, 0x00CE
    /// (an error: "Tell Failed", an unknown command) red.
    Chat(String, [f32; 4]),
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
    /// The live game's look is in force ([`OP_LIVE_LOOK_IN_FORCE`] said so):
    /// character lists are read as the live CLIENT's, and the entity channels
    /// of its length too (`entity::Entities::live_look`). Kept for the
    /// session, as the server keeps it, through the world's connection.
    pub live_look: bool,
    user: String,
    token: [u8; 32],
    start: Instant,
    base: u32,
    /// Datagrams sent so far: `usocket_sendto_addr` draws newlib's `rand()`
    /// twice for each, so a client keeping animation in step draws as many.
    pub packets_sent: u64,
    /// Datagrams read so far, and when `pump` last read one: what a stall's
    /// line says of the server (the client's `stall`: still sending while
    /// the client was held, or gone quiet before).
    pub datagrams_in: u64,
    pub last_datagram: Option<Instant>,
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
    /// 0x0790's probe (`m_p4F0F0`, `m_p4F0F8`, `m_p4F100`, `m_p4F108`),
    /// clear from `Connect` on.
    pub probe: Probe,
}

/// 0x0790's probe: `Msg0790` keeps an endpoint, a token and eight more
/// bytes; `Msg0790Done`, before each of the frame's flushes, sends 0x0992
/// (the token and the bytes) outside any connection to that endpoint, at
/// most once a connection timeout (a second at most), for as long as the
/// session lasts; and `Msg0993` answers the token coming back (a DRDP type
/// 0xF8: a datagram outside any connection) with 0x0791 on the game
/// connection. No server we know sends 0x0790 (none in Sandstorm's
/// captures, none in go-eqoa-server).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Probe {
    /// `m_p4F0F0`: the endpoint 0x0992 goes to; -1 none (`Connect`'s)
    pub endpoint: i64,
    /// `m_p4F0F8`: the token
    pub token: u32,
    /// `m_p4F100`: the eight bytes after it
    pub data: u64,
    /// `m_p4F108`: when 0x0992 last went, on `timer`'s clock; -1 never
    pub last: i64,
}

impl Default for Probe {
    fn default() -> Self {
        Probe { endpoint: -1, token: 0, data: 0, last: -1 }
    }
}

/// Where the DRDP clock (`Session::now`) starts: the wall clock's
/// milliseconds, their low 30 bits. As the whole u32 the clock passed 2^31
/// at 2026-10-28 00:54:06 UTC, and the loading screen's gears, an animation
/// begun at 0 on it, stopped every client entering the world; it would have
/// wrapped at 2026-11-21 21:25:29.7 UTC, and every timer on it that
/// subtracts as an i64 (the loading screen's, the alarm's, the windows')
/// would have stalled in a session across that moment. From here it stays
/// below 2^31 for 12 days of a session and never wraps, as the console's,
/// which is its own clock since boot. The low 16 bits, the client's drdp
/// port (`drdp_create_addr`), are the wall clock's as before, and the
/// instance (the clock at creation) keeps 30 of its bits.
pub fn clock_base(wall_ms: u128) -> u32 {
    (wall_ms % (1 << 30)) as u32
}

fn ipv4(a: &SocketAddr) -> [u8; 4] {
    match a {
        SocketAddr::V4(v) => v.ip().octets(),
        SocketAddr::V6(_) => [0; 4],
    }
}

impl Session {
    /// `VIClient::Connect`: a DRDP client on a fresh UDP socket, a connection
    /// to the login endpoint, the hello and 0x0904. A server at an IPv6
    /// address (a name with no IPv4 one, `addr::resolve`) is reached through
    /// the network's gateway to IPv4, as is every shard after it.
    pub fn connect(server: SocketAddr, user: &str, token: [u8; 32]) -> std::io::Result<Self> {
        let (nat64, server_ip) = match Nat64::of(&server) {
            Some((n, ip)) => (Some(n), ip),
            None => (None, ipv4(&server)),
        };
        let any = if nat64.is_some() { "[::]:0" } else { "0.0.0.0:0" };
        let socket = UdpSocket::bind(any)?;
        // The local address as the server will see it: route a probe.
        let probe = UdpSocket::bind(any)?;
        probe.connect(server)?;
        let local = SocketAddr::new(probe.local_addr()?.ip(), socket.local_addr()?.port());
        socket.set_nonblocking(true)?;
        let udp = Udp { socket, buf: vec![0u8; 65536], nat64 };
        Self::connect_with(Box::new(udp), (ipv4(&local), local.port()), (server_ip, server.port()), user, token)
            .ok_or_else(|| std::io::Error::other("connection_open failed"))
    }

    /// `connect` over any datagram carrier: `local` is the address the
    /// server will see the client at (a relay's own, through one).
    pub fn connect_with(socket: Box<dyn Datagrams>, local: ([u8; 4], u16), server: ([u8; 4], u16), user: &str, token: [u8; 32]) -> Option<Self> {
        let base = clock_base(SystemTime::now().duration_since(UNIX_EPOCH).map_or(1, |d| d.as_millis()));
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
            live_look: false,
            user: user.to_string(),
            token,
            start: Instant::now(),
            base,
            packets_sent: 0,
            datagrams_in: 0,
            last_datagram: None,
            debug: std::env::var_os("EQOA_NET_DEBUG").is_some(),
            chat_mode: 0,
            held: Vec::new(),
            clock_a: SyncTime::default(),
            clock_b: SyncTime::default(),
            record: None,
            probe: Probe::default(),
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

    /// [`OP_ENTITY_VIEW`] to the server: how many entity slots this client
    /// keeps and how far it draws them (a server that offered them fills that
    /// many from that far).
    pub fn send_entity_view(&mut self, slots: u32, view: u32) {
        let mut w = Writer::op(OP_ENTITY_VIEW);
        w.u32(slots).u32(view);
        self.send(&w.data);
    }

    /// `SendOpcodeByte(op, v)`.
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

    /// The game connection as a report's trace gives it: the datagrams sent,
    /// the connection's own counters (`VINetStats`' eleven, those not 0), and
    /// how far the server has acknowledged the 0x40 (state channel 0x40, sent
    /// to and acknowledged to): what tells a server that has the player's
    /// 0x40s but leaves the player out of its world from a link that stopped.
    pub fn net_line(&self) -> String {
        let Some(c) = self.conn.filter(|&c| self.drdp.conns.contains(c)) else {
            return format!("net: {} datagrams sent, no game connection", self.packets_sent);
        };
        let k = &self.drdp.conns[c];
        let counts: Vec<String> = k.cstat.iter().zip(drdp::CSTAT_LABELS).filter(|(n, _)| **n != 0).map(|(n, label)| format!("{n} {label}")).collect();
        let own = k.channels.as_ref().and_then(|v| v.get(0x40)).map_or("; no 0x40 sent".to_string(), |ch| format!("; the 0x40 sent to {}, acknowledged to {}", ch.tx.wrapping_sub(1), ch.tx_ack));
        format!("net: {} datagrams sent; the game connection: {}{own}", self.packets_sent, counts.join(", "))
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
            self.datagrams_in += 1;
            self.last_datagram = Some(Instant::now());
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
        // TickNet and Process: Msg0790Done, then drdp_flush
        self.msg_0790_done();
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
            // ProcessMessageF8: a datagram outside any connection (drdp
            // port 0xffff) comes as type 0xF8; its 0x0993 answers the probe
            0xF8 => self.process_f8(&m),
            _ => {}
        }
    }

    /// `ProcessMessageF8` and `Msg0993`: the probe's token coming back is
    /// answered with 0x0791 and the token, on the game connection.
    fn process_f8(&mut self, m: &drdp::Message) {
        let mut r = Reader::new(&m.data);
        if r.u16() == 0x0993 {
            let token = r.u32();
            if token == self.probe.token {
                let mut w = Writer::op(0x0791);
                w.u32(token);
                self.send(&w.data);
            }
        }
    }

    /// `Msg0790Done`: with a probe set and a game connection, 0x0992 (the
    /// token, the eight bytes) to the probe's endpoint outside any
    /// connection (`VINetSendTo`: `drdp_send_addr` to the endpoint's
    /// address, its drdp port unused), when the connection's timeout (a
    /// second at most) has gone by since the last, on the game's clock.
    fn msg_0790_done(&mut self) {
        let Some(c) = self.conn.filter(|&c| self.drdp.conns.contains(c)) else { return };
        let timeout = (self.drdp.conns[c].timeout as i64).min(1000);
        let now = self.timer();
        if self.probe.endpoint != -1 && now >= self.probe.last + timeout {
            let mut w = Writer::op(0x0992);
            w.u32(self.probe.token).u64(self.probe.data);
            let (to, _) = drdp::endpoint_to_address(self.probe.endpoint as u64);
            self.drdp.send_addr(&[to], &w.data);
            self.probe.last = now;
        }
    }

    /// `Msg07E0`: 0x07E1, the game connection's eleven counters as text
    /// (`VINetStats`: "%I: %s\n" each, the count as a signed 64-bit
    /// number), then whatever followed 0x07E0, on the game connection. Both
    /// Sandstorm and go-eqoa-server send 0x07E0 once a session, and a PS2
    /// answers (`pcaps/ps2_sandstorm.pcap`).
    fn msg_07e0(&mut self, rest: &[u8]) {
        let Some(c) = self.conn.filter(|&c| self.drdp.conns.contains(c)) else { return };
        let cstat = self.drdp.conns[c].cstat;
        let text: String = cstat.iter().zip(drdp::CSTAT_LABELS).map(|(&n, label)| format!("{}: {label}\n", n as i64)).collect();
        let mut w = Writer::op(0x07E1);
        w.wide(&text).bytes(rest);
        self.send(&w.data);
    }

    /// `ProcessMessage`: one opcode.
    fn dispatch(&mut self, m: &drdp::Message, ev: &mut Vec<Event>) {
        let mut r = Reader::new(&m.data);
        let op = r.u16();
        match op {
            0x0000 => ev.push(Event::Version(r.u32())),
            0x07E0 => self.msg_07e0(&m.data[2.min(m.data.len())..]),
            0x0790 => {
                // Msg0790: the probe, sent at once if due
                let endpoint = r.u64() as i64;
                let token = r.u32();
                let data = r.u64();
                self.probe = Probe { endpoint, token, data, last: -1 };
                self.msg_0790_done();
            }
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
            // the trigger names offered: taken (the TEST menu shows them)
            OP_GM_TRIGGERS => {
                if r.u32() != 0 && !r.error {
                    self.send_op_int(OP_GM_TRIGGERS, 1);
                }
            }
            // the live duel's messages offered: taken (the client has the
            // window, `duel`)
            OP_DUEL_WINDOW => {
                if r.u32() != 0 && !r.error {
                    self.send_op_int(OP_DUEL_WINDOW, 1);
                }
            }
            // the live game's look offered: taken (this client reads it),
            // unless a test's `EQOA_LIVE_LOOK=0` keeps the disc's
            OP_LIVE_LOOK => {
                if r.u32() & 1 != 0 && !r.error && wants_live_look() {
                    self.send_op_int(OP_LIVE_LOOK, 1);
                }
            }
            OP_LIVE_LOOK_IN_FORCE => {
                let v = r.u32();
                if !r.error {
                    self.live_look = v & 1 != 0;
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
                ev.push(Event::Characters(read_characters(&mut r, self.live_look)));
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
                r.u32();
                ev.push(Event::Chat(r.wide(1024), [0.5, 0.5, 1.0, 1.0]));
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

/// Whether to take the live game's look when a server offers it: always, but
/// for a test's `EQOA_LIVE_LOOK=0`, which keeps the disc's (a PS2's view of
/// the same server).
fn wants_live_look() -> bool {
    std::env::var("EQOA_LIVE_LOOK").map_or(true, |v| v != "0")
}

/// `CharacterList`: the disc's records, or with `live` the live game's
/// ([`LIVE_GEAR_SIZE`] raw bytes each).
fn read_characters(r: &mut Reader, live: bool) -> Vec<Character> {
    let gear = if live { LIVE_GEAR_SIZE } else { DISC_GEAR_SIZE };
    let n = r.signed().clamp(0, 8) as usize;
    (0..n)
        .map(|_| {
            let name = String::from_utf8_lossy(&r.string(24)).into_owned();
            let mut f = [0i64; 9];
            for v in &mut f {
                *v = r.signed();
            }
            let gear = r.bytes(gear).to_vec();
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

    /// A session through the gateway to IPv4 (`addr::Nat64`), over real
    /// sockets on the IPv6 loopback: a server at [::1] is IPv4 0.0.0.1 under
    /// the prefix ::/96; a datagram to 0.0.0.1 reaches it, and its answer
    /// comes back as from 0.0.0.1. (Skipped where there is no IPv6 loopback.)
    #[test]
    fn a_datagram_goes_through_the_gateway_and_back() {
        let Ok(server) = UdpSocket::bind("[::1]:0") else { return };
        let at = server.local_addr().unwrap();
        let (n, ip) = Nat64::of(&at).unwrap();
        assert_eq!(ip, [0, 0, 0, 1]);
        let socket = UdpSocket::bind("[::]:0").unwrap();
        let port = socket.local_addr().unwrap().port();
        socket.set_nonblocking(true).unwrap();
        let mut udp = Udp { socket, buf: vec![0u8; 2048], nat64: Some(n) };
        udp.send_to(b"hello", ip, at.port());
        server.set_read_timeout(Some(std::time::Duration::from_secs(2))).unwrap();
        let mut b = [0u8; 16];
        let (k, from) = server.recv_from(&mut b).unwrap();
        assert_eq!((&b[..k], from.port()), (&b"hello"[..], port));
        server.send_to(b"answer", from).unwrap();
        let mut got = None;
        for _ in 0..200 {
            if let Some(d) = udp.recv_from() {
                got = Some(d);
                break;
            }
            std::thread::sleep(std::time::Duration::from_millis(5));
        }
        assert_eq!(got, Some((b"answer".to_vec(), [0, 0, 0, 1], at.port())));
    }
    /// The DRDP clock starts low: the wall clock at 2026-10-28 00:58 UTC,
    /// past 2^31 as a u32, starts it below 2^30, so 12 days of a session
    /// stay below 2^31 and it never wraps, its low 16 bits (the drdp port)
    /// the wall clock's as before; any wall clock does. The control: the
    /// whole u32 started past 2^31.
    #[test]
    fn the_clock_starts_below_2_pow_30() {
        let today: u128 = 1_793_149_080_000;
        let b = super::clock_base(today);
        assert!(b < 1 << 30, "{b}");
        assert_eq!(b & 0xFFFF, today as u32 & 0xFFFF, "the port's bits");
        assert!(b + 12 * 24 * 3_600_000 < 1 << 31, "12 days");
        assert!(today as u32 >= 1 << 31, "the control: the u32 itself");
        for wall in [0u128, 1, (1 << 30) - 1, 1 << 30, 1 << 31, (1 << 32) - 1, 1 << 32, u64::MAX as u128, u128::MAX] {
            assert!(super::clock_base(wall) < 1 << 30, "{wall}");
        }
    }

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

    // 0x069E, the group's chat (a u32, then the line), is a chat line in
    // blue, as CLIENT's dispatcher adds it in (.5, .5, 1). The control: the
    // same words on 0x0A7A are white.
    #[test]
    fn a_group_line_is_a_blue_chat_line() {
        let text = "Dairine tells the group: meet at the west gate";
        let mut v = 0x069Eu16.to_le_bytes().to_vec();
        v.extend(0x0001_4F51u32.to_le_bytes());
        v.extend((text.encode_utf16().count() as u32).to_le_bytes());
        v.extend(text.encode_utf16().flat_map(|c| c.to_le_bytes()));
        let mut s = session();
        let ev = deliver(&mut s, drdp::TYPE_GUARANTEED, v);
        assert!(matches!(&ev[..], [Event::Chat(t, c)] if t == text && *c == [0.5, 0.5, 1.0, 1.0]), "{ev:?}");
        let mut w = 0x0A7Au16.to_le_bytes().to_vec();
        w.extend((text.encode_utf16().count() as u32).to_le_bytes());
        w.extend(text.encode_utf16().flat_map(|c| c.to_le_bytes()));
        let ev = deliver(&mut s, drdp::TYPE_GUARANTEED, w);
        assert!(matches!(&ev[..], [Event::Chat(t, c)] if t == text && *c == [1.0; 4]), "the control: {ev:?}");
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

    // A report's net line: the datagrams sent, the game connection's counters
    // not 0, and the 0x40 channel sent to and acknowledged to (a 0x40 sent and
    // flushed is sent to 1, acknowledged to 0 until the server's ack; none
    // before the first). The control: with no game connection, it says so.
    #[test]
    fn the_net_line_says_how_far_the_0x40_is_acknowledged() {
        let mut s = session();
        let before = s.net_line();
        assert!(before.ends_with("; no 0x40 sent"), "{before}");
        s.send_state(0x40, &[1, 2, 3]);
        s.flush();
        let line = s.net_line();
        assert!(line.starts_with(&format!("net: {} datagrams sent; the game connection: ", s.packets_sent)), "{line}");
        assert!(line.contains(" sent") && line.contains("the 0x40 sent to 1, acknowledged to 0"), "{line}");
        s.conn = None;
        assert_eq!(s.net_line(), format!("net: {} datagrams sent, no game connection", s.packets_sent));
    }

    // 0x07E0 is answered with 0x07E1: the game connection's eleven counters
    // as VINetStats writes them, then what followed 0x07E0. The counters of
    // the PS2's own answer in pcaps/ps2_sandstorm.pcap (packet 115, 512
    // bytes after the opcode, a count of 254 characters) give that answer's
    // text exactly. Controls: trailing bytes are echoed after the text, and
    // with no game connection nothing is sent.
    #[test]
    fn stats_are_answered_as_a_ps2_answers_them() {
        let mut s = session();
        s.record = Some(Vec::new());
        let c = s.conn.unwrap();
        s.drdp.conns[c].cstat = [0, 0, 0, 0, 0, 62, 19, 148, 290, 24, 56];
        deliver(&mut s, drdp::TYPE_GUARANTEED, vec![0xE0, 0x07]);
        let sent = s.record.take().unwrap();
        assert_eq!(sent.len(), 1, "{sent:02x?}");
        let (ty, d) = &sent[0];
        assert_eq!(*ty, drdp::TYPE_GUARANTEED);
        assert_eq!(d[..2], [0xE1, 0x07]);
        assert_eq!(d.len() - 2, 512, "the capture's payload size");
        let text = "0: discarded (old)\n0: discarded (no memory was available to process message)\n0: discarded (corrupt)\n0: discarded (crc)\n0: out of sequence\n62: recv\n19: sent\n148: guaranteed recv (duplicates)\n290: guaranteed recv\n24: state recv (duplicates)\n56: state recv\n";
        assert_eq!(d[2..6], 254u32.to_le_bytes(), "the capture's count");
        let wide: Vec<u8> = text.encode_utf16().flat_map(|c| c.to_le_bytes()).collect();
        assert_eq!(d[6..], wide[..]);

        // control: what followed the opcode comes after the text
        s.record = Some(Vec::new());
        deliver(&mut s, drdp::TYPE_GUARANTEED, vec![0xE0, 0x07, 1, 2, 3]);
        let sent = s.record.take().unwrap();
        assert_eq!(sent[0].1[sent[0].1.len() - 3..], [1, 2, 3]);
        assert_eq!(sent[0].1.len(), 2 + 512 + 3);

        // control: no game connection, nothing sent
        let mut s = session();
        s.conn = None;
        s.record = Some(Vec::new());
        deliver(&mut s, drdp::TYPE_GUARANTEED, vec![0xE0, 0x07]);
        assert_eq!(s.record.unwrap(), []);
    }

    // 0x0790's probe: 0x0992 (the token, the eight bytes) outside any
    // connection to the endpoint 0x0790 names, at once and then no more
    // often than the connection's timeout (a second); 0x0993 coming back
    // as a type 0xF8 with the token is answered with 0x0791 on the game
    // connection. Controls: no probe, no 0x0992; a second flush within the
    // timeout sends none; a wrong token is not answered.
    #[test]
    fn a_probe_is_sent_and_its_echo_answered() {
        let mut s = session();
        // the game's clock well on, as it is in play
        s.start -= std::time::Duration::from_secs(20);
        s.drdp.outbox.clear();
        s.msg_0790_done();
        assert!(s.drdp.outbox.is_empty(), "control: no probe, no 0x0992");

        let endpoint = address_to_endpoint([10, 0, 0, 9], 5555, 0x1234);
        let mut w = Writer::op(0x0790);
        w.u64(endpoint).u32(0xAABB_CCDD).u64(0x1122_3344_5566_7788);
        deliver(&mut s, drdp::TYPE_GUARANTEED, w.data);
        assert_eq!(s.probe.endpoint, endpoint as i64);
        let probes: Vec<_> = s.drdp.outbox.drain(..).collect();
        assert_eq!(probes.len(), 1, "sent at once: {probes:02x?}");
        assert_eq!(probes[0].to, drdp::Addr { ip: [10, 0, 0, 9], port: 5555 });
        let mut want = vec![0xff, 0xff, 0x92, 0x09];
        want.extend(0xAABB_CCDDu32.to_le_bytes());
        want.extend(0x1122_3344_5566_7788u64.to_le_bytes());
        assert_eq!(probes[0].data, want, "drdp port 0xffff, no CRC");

        s.msg_0790_done();
        assert!(s.drdp.outbox.is_empty(), "control: not again within the timeout");
        s.start -= std::time::Duration::from_millis(1100);
        s.msg_0790_done();
        assert_eq!(s.drdp.outbox.drain(..).count(), 1, "again a timeout on");

        s.record = Some(Vec::new());
        let mut w = Writer::op(0x0993);
        w.u32(0x1234_5678);
        deliver(&mut s, 0xF8, w.data);
        assert_eq!(s.record.as_deref(), Some(&[][..]), "control: a wrong token");
        let mut w = Writer::op(0x0993);
        w.u32(0xAABB_CCDD);
        deliver(&mut s, 0xF8, w.data);
        assert_eq!(s.record.unwrap(), [(drdp::TYPE_GUARANTEED, vec![0x91, 0x07, 0xDD, 0xCC, 0xBB, 0xAA])]);
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

    // go-eqoa-server's offers and sets past 0x7E00 (0x7E01-0x7E06) are
    // opcodes this client has no case for, as CLIENT's dispatcher has none:
    // each passed on as one (play reads none of them) and nothing answered.
    // The control: the same look finds the answer to 0x7E00's offer.
    #[test]
    fn the_servers_other_offers_go_unanswered() {
        let sent = std::rc::Rc::new(std::cell::RefCell::new(Vec::new()));
        let mut s = Session::connect_with(Box::new(Rec(sent.clone())), ([127, 0, 0, 1], 40000), ([127, 0, 0, 1], 10070), "test", [0; 32]).unwrap();
        let answers = |s: &mut Session| -> Vec<(u8, Vec<u8>)> {
            s.flush();
            let m = sent.borrow().iter().flat_map(|d| messages(d)).filter(|(ty, d)| *ty == 251 && d.get(1) == Some(&0x7E)).collect();
            sent.borrow_mut().clear();
            m
        };
        answers(&mut s);
        for op in [0x7E01u16, 0x7E02, 0x7E03, 0x7E04, 0x7E05, 0x7E06] {
            let mut m = op.to_le_bytes().to_vec();
            m.extend([1, 0, 0, 0]);
            let ev = deliver(&mut s, drdp::TYPE_GUARANTEED, m);
            assert!(matches!(&ev[..], [Event::Opcode { op: o, .. }] if *o == op), "{op:#06x}: {ev:?}");
        }
        assert_eq!(answers(&mut s), [], "nothing answered");
        deliver(&mut s, drdp::TYPE_GUARANTEED, vec![0x00, 0x7E, 204, 0, 0, 0, 0xE8, 0x03, 0, 0]);
        assert_eq!(s.entity_view_offer, Some((204, 1000)), "0x7E00's offer still taken");
        s.send_entity_view(24, 100);
        assert_eq!(answers(&mut s), [(251, vec![0x00, 0x7E, 24, 0, 0, 0, 100, 0, 0, 0])], "the control: 0x7E00's answer");
    }

    // go-eqoa-server's offer of the live game's look (0x7E0C) is answered
    // with 1 at once; the server's answer (0x7E0D) decides how the character
    // lists after it are read: the disc's 32 raw bytes a record until it says
    // 1, the live game's 71 after, the disc's again after a 0. It is kept
    // through the world's connection, as the server keeps it for the
    // session. The control: a list that comes before the server's answer is
    // the disc's, though this client asked already.
    #[test]
    fn live_look_offer_answer_and_lists() {
        let sent = std::rc::Rc::new(std::cell::RefCell::new(Vec::new()));
        let mut s = Session::connect_with(Box::new(Rec(sent.clone())), ([127, 0, 0, 1], 40000), ([127, 0, 0, 1], 10070), "test", [0; 32]).unwrap();
        let answers = |s: &mut Session| -> Vec<(u8, Vec<u8>)> {
            s.flush();
            let m = sent.borrow().iter().flat_map(|d| messages(d)).filter(|(ty, d)| *ty == 251 && d.get(1) == Some(&0x7E)).collect();
            sent.borrow_mut().clear();
            m
        };
        let list = |gear: usize| {
            let mut w = Writer::op(0x002C);
            w.signed(1).u32(4).bytes(b"Dyed");
            for v in [17, 0x70D8_98C6, 13, 3, 42, 1, 2, 3, 2] {
                w.signed(v);
            }
            w.bytes(&vec![0xAB; gear]);
            w.data
        };
        let gear_of = |ev: &[Event]| match ev {
            [Event::Characters(c)] => c.iter().map(|c| c.gear.len()).collect::<Vec<_>>(),
            other => panic!("{other:?}"),
        };
        answers(&mut s);
        s.state = State::Shard;
        assert_eq!(gear_of(&deliver(&mut s, drdp::TYPE_GUARANTEED, list(DISC_GEAR_SIZE))), [32], "a first list: the disc's");
        assert!(deliver(&mut s, drdp::TYPE_GUARANTEED, vec![0x0C, 0x7E, 1, 0, 0, 0]).is_empty());
        assert_eq!(answers(&mut s), [(251, vec![0x0C, 0x7E, 1, 0, 0, 0])], "the offer answered with 1");
        assert!(!s.live_look, "not in force until the server says so");
        assert_eq!(gear_of(&deliver(&mut s, drdp::TYPE_GUARANTEED, list(DISC_GEAR_SIZE))), [32], "the control: a list before the server's answer");

        deliver(&mut s, drdp::TYPE_GUARANTEED, vec![0x0D, 0x7E, 1, 0, 0, 0]);
        assert!(s.live_look);
        let ev = deliver(&mut s, drdp::TYPE_GUARANTEED, list(LIVE_GEAR_SIZE));
        assert_eq!(gear_of(&ev), [71], "the live game's list");
        if let [Event::Characters(c)] = &ev[..] {
            assert_eq!((c[0].name.as_str(), c[0].level, c[0].face), ("Dyed", 42, 2));
        }
        deliver(&mut s, drdp::TYPE_CONNECT, Vec::new());
        assert!(s.live_look, "kept through the world's connection");
        deliver(&mut s, drdp::TYPE_GUARANTEED, vec![0x0D, 0x7E, 0, 0, 0, 0]);
        assert!(!s.live_look, "the server went back to the disc's");
        assert_eq!(answers(&mut s), [], "nothing answered but the offer");
    }

    // go-eqoa-server's offer of the GM's trigger names (0x7E07) is answered
    // with 1 at once, every time it comes, and its names (0x7E08) are passed
    // on for the TEST menu with nothing answered. The controls: an offer cut
    // short and an offer of 0 are not answered.
    #[test]
    fn the_trigger_names_offer_is_answered() {
        let sent = std::rc::Rc::new(std::cell::RefCell::new(Vec::new()));
        let mut s = Session::connect_with(Box::new(Rec(sent.clone())), ([127, 0, 0, 1], 40000), ([127, 0, 0, 1], 10070), "test", [0; 32]).unwrap();
        let answers = |s: &mut Session| -> Vec<(u8, Vec<u8>)> {
            s.flush();
            let m = sent.borrow().iter().flat_map(|d| messages(d)).filter(|(ty, d)| *ty == 251 && d.get(1) == Some(&0x7E)).collect();
            sent.borrow_mut().clear();
            m
        };
        answers(&mut s);
        let ev = deliver(&mut s, drdp::TYPE_GUARANTEED, vec![0x07, 0x7E, 1, 0, 0]);
        assert!(ev.is_empty(), "{ev:?}");
        assert_eq!(answers(&mut s), [], "the control: an offer cut short");
        deliver(&mut s, drdp::TYPE_GUARANTEED, vec![0x07, 0x7E, 0, 0, 0, 0]);
        assert_eq!(answers(&mut s), [], "the control: an offer of 0");
        let ev = deliver(&mut s, drdp::TYPE_GUARANTEED, vec![0x07, 0x7E, 1, 0, 0, 0]);
        assert!(ev.is_empty(), "the offer is the session's: {ev:?}");
        assert_eq!(answers(&mut s), [(251, vec![0x07, 0x7E, 1, 0, 0, 0])]);
        deliver(&mut s, drdp::TYPE_GUARANTEED, vec![0x07, 0x7E, 1, 0, 0, 0]);
        assert_eq!(answers(&mut s), [(251, vec![0x07, 0x7E, 1, 0, 0, 0])], "again for another offer");
        let names = vec![0x08, 0x7E, 1, 2, 1, 3, b'O', b'n', b'e'];
        let ev = deliver(&mut s, drdp::TYPE_GUARANTEED, names);
        assert!(matches!(&ev[..], [Event::Opcode { op: 0x7E08, data }] if data[..] == [1, 2, 1, 3, b'O', b'n', b'e']), "{ev:?}");
        assert_eq!(answers(&mut s), [], "the names answered nothing");
    }

    // go-eqoa-server's offer of the live duel's messages (0x7E09) is answered
    // with 1 at once, every time it comes, and passed on as nothing. The
    // controls: an offer cut short and an offer of 0 are not answered.
    #[test]
    fn the_duel_window_offer_is_answered() {
        let sent = std::rc::Rc::new(std::cell::RefCell::new(Vec::new()));
        let mut s = Session::connect_with(Box::new(Rec(sent.clone())), ([127, 0, 0, 1], 40000), ([127, 0, 0, 1], 10070), "test", [0; 32]).unwrap();
        let answers = |s: &mut Session| -> Vec<(u8, Vec<u8>)> {
            s.flush();
            let m = sent.borrow().iter().flat_map(|d| messages(d)).filter(|(ty, d)| *ty == 251 && d.get(1) == Some(&0x7E)).collect();
            sent.borrow_mut().clear();
            m
        };
        answers(&mut s);
        let ev = deliver(&mut s, drdp::TYPE_GUARANTEED, vec![0x09, 0x7E, 1, 0, 0]);
        assert!(ev.is_empty(), "{ev:?}");
        assert_eq!(answers(&mut s), [], "the control: an offer cut short");
        deliver(&mut s, drdp::TYPE_GUARANTEED, vec![0x09, 0x7E, 0, 0, 0, 0]);
        assert_eq!(answers(&mut s), [], "the control: an offer of 0");
        let ev = deliver(&mut s, drdp::TYPE_GUARANTEED, vec![0x09, 0x7E, 1, 0, 0, 0]);
        assert!(ev.is_empty(), "the offer is the session's: {ev:?}");
        assert_eq!(answers(&mut s), [(251, vec![0x09, 0x7E, 1, 0, 0, 0])]);
        deliver(&mut s, drdp::TYPE_GUARANTEED, vec![0x09, 0x7E, 1, 0, 0, 0]);
        assert_eq!(answers(&mut s), [(251, vec![0x09, 0x7E, 1, 0, 0, 0])], "again for another offer (each world entry)");
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
