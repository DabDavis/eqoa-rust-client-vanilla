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

#[derive(Clone, Debug)]
pub enum Event {
    /// 0x0000: the server's version.
    Version(u32),
    ShardList(Vec<Shard>),
    Characters(Vec<Character>),
    /// 0x0013: the clock pair (`SetTime`).
    Time(u64, u64),
    /// 0x07D1: the expansion-flag mask.
    Expansion(u32),
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
    socket: UdpSocket,
    pub conn: Option<ConnId>,
    pub state: State,
    pub self_id: Option<u64>,
    user: String,
    token: [u8; 32],
    start: std::time::Instant,
    base: u32,
    /// Datagrams sent so far: `usocket_sendto_addr` draws newlib's `rand()`
    /// twice for each, so a client keeping animation in step draws as many.
    pub packets_sent: u64,
    /// `EQOA_NET_DEBUG`: every datagram and message on stderr.
    debug: bool,
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
        let base = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map(|d| d.as_millis() as u32).unwrap_or(1);
        let mut drdp = Drdp::<Ee>::create_client(Addr { ip: ipv4(&local), port: local.port() }, base);
        drdp.set_port_udp(local.port());
        let mut s = Session {
            drdp,
            socket,
            conn: None,
            state: State::Login,
            self_id: None,
            user: user.to_string(),
            token,
            start: std::time::Instant::now(),
            base,
            packets_sent: 0,
            debug: std::env::var_os("EQOA_NET_DEBUG").is_some(),
        };
        let dst = address_to_endpoint(ipv4(&server), server.port(), PORT_LEADER);
        let sys = s.drdp.system_endpoint;
        let c = s.drdp.connection_open(sys, dst).ok_or_else(|| std::io::Error::other("connection_open failed"))?;
        s.drdp.connection_set_failure(c, 1000, 30000, 60000);
        s.drdp.connection_set_simulated(c, 0, 0, 0);
        s.conn = Some(c);
        s.send_hello();
        let mut w = Writer::op(0x0904);
        w.u8(0).u32(EXPANSION_FLAGS);
        s.send_login(w);
        Ok(s)
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

    /// One frame: the clock, every datagram waiting, the arrived messages
    /// (as events), a flush, and the sends.
    pub fn pump(&mut self) -> Vec<Event> {
        let t = self.now();
        self.drdp.set_time(t);
        let mut buf = vec![0u8; 65536];
        while let Ok((n, p)) = self.socket.recv_from(&mut buf) {
            let mut d = ipv4(&p).to_vec();
            d.extend(p.port().to_le_bytes());
            d.extend(&buf[..n]);
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
        let mut events = Vec::new();
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
            let _ = self.socket.send_to(&p.data, SocketAddr::from((p.to.ip, p.to.port)));
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
            0x07D1 => ev.push(Event::Expansion(r.u32())),
            0x0013 => {
                let (a, b) = (r.u64(), r.u64());
                ev.push(Event::Time(a, b));
            }
            0x002C if matches!(self.state, State::Shard | State::Characters) => {
                self.state = State::Characters;
                ev.push(Event::Characters(read_characters(&mut r)));
            }
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
            0x0046 => {
                let id = r.u32();
                let n = r.u32() as usize;
                let raw = r.bytes(n * 2);
                // ReadString(buf, 1024): 511 characters fit.
                let text = if n < 512 { raw.as_chunks::<2>().0.iter().map(|c| u16::from_le_bytes(*c)).collect() } else { Vec::new() };
                ev.push(Event::Dialog { id, text });
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

