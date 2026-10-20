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
    /// heading, pitch and roll, and the token 0x0049 answers with.
    Teleport { world: u8, pos: [f32; 3], hpr: [f32; 3], token: u32 },
    /// 0x0020: the player's level and experience.
    LevelExp { level: i64, exp: i64 },
    /// 0x0775: an entity plays an action (`VICSprite::SetAction`).
    Action { id: u32, action: u8 },
    /// 0x07A3: who last sent a tell (`/reply` answers them).
    ReplyName(String),
    /// 0x00D9: the player's +0x2C.
    PlayerP2C(i32),
    /// A line for the chat log, in its colour (RGBA 0..1): 0x0A7A white,
    /// 0x0A7B its own colour, 0x069E blue, 0x0758 (narrow) white.
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
    /// 0xFD: the connection ended, with the library's reason byte.
    Disconnected(u8),
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

pub struct Session {
    pub drdp: Drdp<Ee>,
    socket: Box<dyn Datagrams>,
    pub conn: Option<ConnId>,
    pub state: State,
    pub self_id: Option<u64>,
    /// 0x07D1's mask (`VIClient+0x50304`), as the server last sent it.
    pub expansion: u32,
    user: String,
    token: [u8; 32],
    start: Instant,
    base: u32,
    /// Datagrams sent so far: `usocket_sendto_addr` draws newlib's `rand()`
    /// twice for each, so a client keeping animation in step draws as many.
    pub packets_sent: u64,
    /// `EQOA_NET_DEBUG`: every datagram and message on stderr.
    debug: bool,
    /// Events handed back to come out of the next `pump` first: what
    /// arrived with the world entry, after it (0x1005, 0x03D5), for the
    /// game to read once it is up.
    pub held: Vec<Event>,
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
            user: user.to_string(),
            token,
            start: Instant::now(),
            base,
            packets_sent: 0,
            debug: std::env::var_os("EQOA_NET_DEBUG").is_some(),
            held: Vec::new(),
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
            self.drdp.connection_send(c, data, drdp::TYPE_GUARANTEED);
        }
    }

    /// A state channel update (0x40 is the player's own state).
    pub fn send_state(&mut self, channel: u8, data: &[u8]) {
        if let Some(c) = self.conn {
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
    pub fn send_op_byte(&mut self, op: u16, v: u8) {
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

    /// `ConnectShard`: the game endpoint, the hello and 0x0001.
    pub fn connect_shard(&mut self, endpoint: u64) {
        self.state = State::Shard;
        if let Some(c) = self.conn.take() {
            self.drdp.connection_close(c);
        }
        let sys = self.drdp.system_endpoint;
        self.conn = self.drdp.connection_open(sys, endpoint);
        self.send_hello();
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
    pub fn now(&self) -> u32 {
        self.base.wrapping_add(self.start.elapsed().as_millis() as u32)
    }

    /// `VIClient::IsTimedOut`: no connection, or nothing has arrived on it
    /// for 8 s.
    pub fn timed_out(&self) -> bool {
        let Some(c) = self.conn.and_then(|c| self.drdp.conns.get(c)) else { return true };
        self.now().wrapping_sub(c.time_last_arrival) >= 8000
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
                    eprintln!("[net]    body head {body:02x?}");
                    b.skip(h.size as usize);
                    segs.push(format!("flags {:#x} size {} crc {:#x} src {:#x} dst {:#x}", h.flags, h.size, h.crc, h.src, h.dst));
                }
                eprintln!("[net] <- {n} bytes, drdp ports {from:#x}->{to:#x}: {r}; {segs:?}");
                let sys = self.drdp.system_endpoint;
                for (&dst, &c) in &self.drdp.eps[sys].connections {
                    let k = &self.drdp.conns[c];
                    eprintln!("[net]    conn {dst:#x}: flags {:#x} flush {} arrival {} rx_seq {} tx_seq {} tx_ack {} inst {:#x}/{:#x} cstat {:?}", k.flags, k.flush, k.arrival_number, k.rx_seqnum, k.tx_seqnum, k.tx_ack, k.instance_local, k.instance_remote, k.cstat);
                }
            }
        }
        let mut events = std::mem::take(&mut self.held);
        while let Some(m) = self.drdp.receive() {
            if self.debug {
                eprintln!("[net] message type {} size {} src {:#x}", m.ty, m.data.len(), m.src);
            }
            self.route(m, &mut events);
        }
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
                eprintln!("[net] -> {} bytes, drdp ports {from:#x}->{to:#x}: {segs:?}", p.data.len());
            }
            self.socket.send_to(&p.data, p.to.ip, p.to.port);
            self.packets_sent += 1;
        }
        events
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
                ev.push(Event::Promoted);
            }
            drdp::TYPE_DISCONNECT => {
                if m.connection == self.conn {
                    self.conn = None;
                }
                ev.push(Event::Disconnected(m.data.first().copied().unwrap_or(0)));
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
            0x0013 => {
                let (a, b) = (r.u64(), r.u64());
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
                    eprintln!("[net] MemoryDump: {} of {} bytes read{}", r.pos, r.data.len(), if r.error { ", SHORT" } else { "" });
                }
                // Msg000D goes on only when the whole dump read.
                if !r.error {
                    let p = &dump.player;
                    let player = Player {
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
                    ev.push(Event::WorldEntry { self_id: m.src, player, dump: Box::new(dump) });
                }
                let mut w = Writer::op(0x00C9);
                w.u8(1).u8(0);
                self.send(&w.data);
                let mut w = Writer::op(0x00CA);
                w.u8(1).u8(0);
                self.send(&w.data);
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
                // SendOpSigned(73, token): answered whatever happens.
                let mut w = Writer::op(0x0049);
                w.signed(token as i32 as i64);
                self.send(&w.data);
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
            0x00D9 => ev.push(Event::PlayerP2C(r.u32() as i32)),
            0x007E => ev.push(Event::Pet(Some(r.u32()))),
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
            _ => ev.push(Event::Opcode { op, data: m.data[2.min(m.data.len())..].to_vec() }),
        }
    }
}

/// `VIShardInfo::Read`: entries for the client's language (0) only.
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

/// `CharacterList`: at most eight.
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

