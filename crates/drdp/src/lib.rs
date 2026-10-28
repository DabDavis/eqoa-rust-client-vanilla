//! DRDP, the game's network library, ported from the matching decomp
//! (`decomp/src/{drdp,connection,endpoint,route,route_cache,route_table,
//! message,close_set,bandwidth,buffer,rle}.cpp`) for the CLIENT role.
//!
//! The library is one `Drdp` that owns every endpoint, connection, route and
//! message, addressed by index where the original holds pointers. Its
//! intrusive trees and lists are ordered vectors kept in the original's
//! order (`list`), so everything that walks them walks the same sequence and
//! every packet comes out byte for byte the same.
//!
//! The socket is not here: a packet to send lands in `Drdp::outbox` (the
//! original calls `usocket_sendto_addr`, which also draws `rand()` twice per
//! packet from newlib's shared generator, so a client that keeps that
//! generator in step must do the same per packet), and a received datagram is
//! handed to `process_packet` behind its sender's address, as
//! `drdp_receive_packet` hands it on.
//!
//! Not ported, and not reachable by a client talking to a server: endpoint
//! transfer and the serialisation under it (`*_create_buffer`,
//! `*_destroy_buffer`), the master and leader roles' endpoint blocks and
//! route-table exchange (system messages 2, 17, 32-34 and 48, which in the
//! original dereference a master connection a client does not have), ICMP
//! (it needs a raw socket), the logger and the debug formatting.

pub mod bandwidth;
pub mod buffer;
mod conn;
mod endpoint;
pub mod list;
mod route;

use bandwidth::Bandwidth;
use buffer::Buffer;
use list::{Arena, cmp_u32_wrap, insert_sorted, remove};
use std::collections::BTreeMap;
use vi::fpu::Model;

pub use conn::{CSTAT_LABELS, Connection, StateChannel};
pub use endpoint::Endpoint;
pub use route::Route;

pub type MsgId = usize;
pub type ConnId = usize;
pub type EpId = usize;
pub type RouteId = usize;

pub const PORT_NONE: u16 = 0xffff;
pub const PORT_LEADER: u16 = 0xfffe;
pub const PORT_UNREACHABLE: u16 = 0xfffd;
pub const SW_ENDPOINT_MIN: u64 = 2048;
pub const SW_ENDPOINT_MAX: u64 = 0xffff_ffff;

pub const TYPE_STATE_CHANNELS: u8 = 248;
pub const TYPE_SYSTEM: u8 = 249;
pub const TYPE_FRAGMENT: u8 = 250;
pub const TYPE_GUARANTEED: u8 = 251;
pub const TYPE_BEST_EFFORT: u8 = 252;
pub const TYPE_DISCONNECT: u8 = 253;
pub const TYPE_CONNECT: u8 = 255;

/// `drdp_crc_table`: the standard CRC-32 table; the packet trailer is the
/// CRC XOR'd with entry `version` (2).
pub static CRC_TABLE: [u32; 256] = {
    let mut t = [0u32; 256];
    let mut i = 0;
    while i < 256 {
        let mut c = i as u32;
        let mut k = 0;
        while k < 8 {
            c = if c & 1 != 0 { 0xedb88320 ^ (c >> 1) } else { c >> 1 };
            k += 1;
        }
        t[i] = c;
        i += 1;
    }
    t
};

/// `drdp_crc`.
pub fn crc(crc: u32, data: &[u8]) -> u32 {
    let mut c = !crc;
    for &b in data {
        c = CRC_TABLE[((c ^ b as u32) & 0xff) as usize] ^ (c >> 8);
    }
    !c
}

/// `drdp_crc_table_lookup`: the table index `value` is, searched outwards
/// from `index`.
pub fn crc_table_lookup(value: u32, index: i32) -> i32 {
    let lo = (index - 4).max(0);
    let hi = (index + 4).min(255);
    if CRC_TABLE[index as usize] == value {
        return index;
    }
    (lo..hi).find(|&i| CRC_TABLE[i as usize] == value).unwrap_or(-1)
}

/// `drdp_addr_t`: four octets and a UDP port.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct Addr {
    pub ip: [u8; 4],
    pub port: u16,
}

/// `drdp_address_to_endpoint`: octets, UDP port and drdp port, big end first.
pub fn address_to_endpoint(ip: [u8; 4], port: u16, drdp_port: u16) -> u64 {
    let mut id = 0u64;
    for b in ip {
        id = (id << 8) | b as u64;
    }
    ((id << 16 | port as u64) << 16) | drdp_port as u64
}

/// `drdp_endpoint_to_address`.
pub fn endpoint_to_address(id: u64) -> (Addr, u16) {
    let drdp_port = id as u16;
    let port = (id >> 16) as u16;
    let ip = ((id >> 32) as u32).to_be_bytes();
    (Addr { ip, port }, drdp_port)
}

pub fn is_hardware_endpoint(id: u64) -> bool {
    id != u64::MAX && (id >> 56) != 0 && id & 0xffff_0000 != 0
}

pub fn is_software_endpoint(id: u64) -> bool {
    id.wrapping_sub(SW_ENDPOINT_MIN) <= SW_ENDPOINT_MAX - SW_ENDPOINT_MIN
}

pub fn is_endpoint(id: u64) -> bool {
    is_hardware_endpoint(id) || is_software_endpoint(id)
}

pub fn is_local_hardware_endpoint(id: u64) -> bool {
    id != u64::MAX && id & 0xffff_ffff_ffff_0000 == 0xffff_ffff_ffff_0000
}

pub fn is_remote_hardware_endpoint(id: u64) -> bool {
    id != u64::MAX && (id >> 56) != 0 && id & 0xffff_ffff_ffff_0000 != 0xffff_ffff_ffff_0000
}

/// `message_t`, header and payload.
#[derive(Clone, Debug, Default)]
pub struct Message {
    pub endpoint: Option<EpId>,
    pub connection: Option<ConnId>,
    pub src: u64,
    pub dst: u64,
    pub data: Vec<u8>,
    pub seqnum: u16,
    pub ty: u8,
    pub refnum_offset: u8,
    pub flush: u16,
    pub created: u32,
    pub time: u32,
}

/// `segment_header_t`.
#[derive(Clone, Copy, Debug, Default)]
pub struct SegmentHeader {
    pub flags: u32,
    pub size: u32,
    pub crc: u32,
    pub flush_number: u32,
    pub route: u64,
    pub src: u64,
    pub dst: u64,
}

/// `buffer_read_segment_header`.
pub fn read_segment_header(b: &mut Buffer, route: u64, dst: u64) -> SegmentHeader {
    let mut h = SegmentHeader { flags: b.packed() as u32, ..Default::default() };
    h.size = h.flags & 0x7ff;
    h.flags &= 0xffff_f800;
    if h.flags & 0x2000 != 0 {
        h.crc = b.u32();
    }
    if h.flags & 0x40000 != 0 {
        h.flush_number = b.u32();
    }
    h.route = if h.flags & 0x8000 != 0 { b.u64() } else { route };
    h.src = if h.flags & 0x1000 != 0 { h.route } else { b.packed() };
    h.dst = if h.flags & 0x800 != 0 { b.packed() } else { dst };
    h
}

/// `buffer_append_segment_header`.
pub fn append_segment_header(b: &mut Buffer, h: &mut SegmentHeader) {
    if is_local_hardware_endpoint(h.src) {
        h.flags |= 0x1000;
    }
    if is_software_endpoint(h.dst) {
        h.flags |= 0x800;
    }
    b.put_packed((h.flags | h.size) as u64);
    if h.flags & 0x2000 != 0 {
        b.put_u32(h.crc);
    }
    if h.flags & 0x40000 != 0 {
        b.put_u32(h.flush_number);
    }
    if h.flags & 0x8000 != 0 {
        b.put_u64(h.route);
    }
    if h.flags & 0x1000 == 0 {
        b.put_packed(h.src);
    }
    if h.flags & 0x800 != 0 {
        b.put_packed(h.dst);
    }
}

/// One datagram for the socket.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Packet {
    pub to: Addr,
    pub data: Vec<u8>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Role {
    Master = 0,
    Leader = 1,
    Client = 2,
}

/// `close_t` and `close_set_t`: recently closed (endpoint, instance) pairs,
/// in expiry order, so a late segment for one is answered with a reset.
#[derive(Clone, Debug, Default)]
pub struct CloseSet {
    /// (id, instance, expiry), sorted by expiry (wrapping).
    list: Vec<(u64, u32, u32)>,
}

impl CloseSet {
    pub fn insert(&mut self, id: u64, instance: u32, time: u32, now: u32) {
        let expire = time.wrapping_add(120000);
        self.release_expired(now);
        if expire.wrapping_sub(now) as i32 >= 0 {
            let mut keys: Vec<usize> = (0..self.list.len()).collect();
            let list = &self.list;
            let new = list.len();
            let mut all = list.clone();
            all.push((id, instance, expire));
            insert_sorted(&mut keys, new, |k| all[k].2 as u64, |a, b| cmp_u32_wrap(a as u32, b as u32));
            self.list = keys.into_iter().map(|k| all[k]).collect();
        }
    }

    pub fn release_expired(&mut self, now: u32) {
        while let Some(&(_, _, t)) = self.list.first() {
            if t.wrapping_sub(now) as i32 > 0 {
                break;
            }
            self.list.remove(0);
        }
    }

    pub fn lookup(&self, id: u64, instance: u32) -> bool {
        self.list.iter().any(|&(i, n, _)| i == id && n == instance)
    }
}

/// `route_cache_t`: address <-> route remapping with a five-minute expiry.
#[derive(Clone, Debug, Default)]
pub struct RouteCache {
    /// (address, route, expiry), in expiry order (wrapping).
    entries: Vec<(Addr, Addr, u32)>,
}

const ROUTE_CACHE_EXPIRE: u32 = 300000;

impl RouteCache {
    fn reinsert(&mut self, i: usize, expire: u32) {
        let mut e = self.entries.remove(i);
        e.2 = expire;
        let mut idx: Vec<usize> = (0..self.entries.len()).collect();
        let mut all = self.entries.clone();
        all.push(e);
        let new = all.len() - 1;
        insert_sorted(&mut idx, new, |k| all[k].2 as u64, |a, b| cmp_u32_wrap(a as u32, b as u32));
        self.entries = idx.into_iter().map(|k| all[k]).collect();
    }

    pub fn release_expired(&mut self, now: u32) {
        while let Some(&(_, _, t)) = self.entries.first() {
            if t.wrapping_sub(now) as i32 > 0 {
                break;
            }
            self.entries.remove(0);
        }
    }

    /// `route_cache_address_to_route`.
    pub fn address_to_route(&mut self, a: &Addr, now: u32) -> Addr {
        match self.entries.iter().position(|e| e.0 == *a) {
            None => *a,
            Some(i) => {
                let r = self.entries[i].1;
                if self.entries[i].2 != now.wrapping_add(ROUTE_CACHE_EXPIRE) {
                    self.reinsert(i, now.wrapping_add(ROUTE_CACHE_EXPIRE));
                }
                r
            }
        }
    }

    /// `route_cache_route_to_address`.
    pub fn route_to_address(&mut self, r: &Addr, now: u32) -> Addr {
        match self.entries.iter().position(|e| e.1 == *r) {
            None => *r,
            Some(i) => {
                let a = self.entries[i].0;
                if self.entries[i].2 != now.wrapping_add(ROUTE_CACHE_EXPIRE) {
                    self.reinsert(i, now.wrapping_add(ROUTE_CACHE_EXPIRE));
                }
                a
            }
        }
    }
}

/// `route_table_t`: which leader owns each software endpoint range.
#[derive(Clone, Debug)]
pub struct RouteTable {
    lo: u64,
    hi: u64,
    /// (lo, hi, owner), sorted and disjoint.
    entries: Vec<(u64, u64, u64)>,
}

impl RouteTable {
    pub fn new(lo: u64, hi: u64) -> Self {
        RouteTable { lo, hi, entries: vec![(lo, hi, 0)] }
    }

    fn find_index(&self, lo: u64, hi: u64) -> Option<usize> {
        // uns64_range_t_cmp: equal when the ranges overlap.
        self.entries.iter().position(|e| !(hi < e.0) && !(e.1 < lo))
    }

    fn insert(&mut self, e: (u64, u64, u64)) {
        let i = self.entries.iter().position(|x| e.1 < x.0).unwrap_or(self.entries.len());
        self.entries.insert(i, e);
    }

    /// `route_table_update_range`.
    pub fn update_range(&mut self, lo: u64, hi: u64, route: u64) {
        let Some(i) = self.find_index(lo, hi) else { return };
        let rte = self.entries[i];
        if rte.2 == route {
            return;
        }
        if rte.0 == lo && rte.1 == hi {
            if route == u64::MAX {
                self.entries.remove(i);
                return;
            }
            self.entries[i].2 = route;
            let mut i = i;
            if i > 0 {
                let prev = self.entries[i - 1];
                if prev.1.wrapping_add(1) == lo && prev.2 == route {
                    self.entries[i].0 = prev.0;
                    self.entries.remove(i - 1);
                    i -= 1;
                }
            }
            if let Some(&next) = self.entries.get(i + 1)
                && hi.wrapping_add(1) == next.0
                && route == next.2
            {
                self.entries[i].1 = next.1;
                self.entries.remove(i + 1);
            }
            return;
        }
        if lo == rte.0 {
            self.entries[i].0 = hi.wrapping_add(1);
            if route == u64::MAX {
                return;
            }
            if i > 0 {
                let left = self.entries[i - 1];
                if left.1.wrapping_add(1) == lo && left.2 == route {
                    self.entries[i - 1].1 = hi;
                    return;
                }
            }
            self.insert((lo, hi, route));
        } else if hi == rte.1 {
            self.entries[i].1 = lo.wrapping_sub(1);
            if route == u64::MAX {
                return;
            }
            if let Some(&right) = self.entries.get(i + 1)
                && hi == right.0.wrapping_sub(1)
                && route == right.2
            {
                self.entries[i + 1].0 = lo;
                return;
            }
            self.insert((lo, hi, route));
        } else {
            self.entries[i].1 = lo.wrapping_sub(1);
            self.insert((hi.wrapping_add(1), rte.1, rte.2));
            if route == u64::MAX {
                return;
            }
            self.insert((lo, hi, route));
        }
    }

    pub fn update(&mut self, id: u64, route: u64) {
        self.update_range(id, id, route);
    }

    pub fn update_block(&mut self, id: u64, route: u64) {
        self.update_range(id, id.wrapping_add(2047), route);
    }

    /// `route_table_find`: `None` where the original returns -1 (outside
    /// the table, or a range nobody owns); `u64::MAX` for no entry.
    pub fn find(&self, id: u64) -> Option<u64> {
        if id < self.lo || id > self.hi {
            return None;
        }
        match self.find_index(id, id) {
            None => Some(u64::MAX),
            Some(i) if self.entries[i].2 == 0 => None,
            Some(i) => Some(self.entries[i].2),
        }
    }
}

/// The source file the original's `connection_terminate_file_line` names.
const DRDP_CPP: &[u8] = b"C:/code-frontiers-gm/drdp/drdp.cpp";

/// `drdp_t`, one process on the network.
pub struct Drdp<M: Model> {
    pub time: u32,
    time_flush: u32,
    time_bps: u32,
    bytes_sent: i32,
    bytes_recv: i32,
    bytes_dropped: i32,
    pub role: Role,
    pub no_send: bool,
    /// `m_bindAddress`.
    pub bind: Addr,
    endpoints: BTreeMap<u64, EpId>,
    /// Routes by desired flush time (wrapping), insert after equals.
    routes_by_time: Vec<RouteId>,
    routes_by_id: BTreeMap<u64, RouteId>,
    /// The process-wide arrival list (each message is also on its endpoint's).
    arrivals: Vec<MsgId>,
    leaders: BTreeMap<u64, Option<ConnId>>,
    /// Every connection, by remote id, insert after equals.
    conns_by_dst: Vec<ConnId>,
    route_table: RouteTable,
    route_cache: RouteCache,
    bandwidth: Bandwidth<M>,
    pub system_endpoint: EpId,
    master_connection: Option<ConnId>,
    pub msgs: Arena<Message>,
    pub eps: Arena<Endpoint>,
    pub conns: Arena<Connection>,
    pub routes: Arena<Route<M>>,
    /// Datagrams to send, in order.
    pub outbox: Vec<Packet>,
}

impl<M: Model> Drdp<M> {
    /// `drdp_create_client`: role client, 3,000 bps, a 300-byte queue. The
    /// system endpoint's drdp port is the low half of the clock, stepped off
    /// the three reserved values.
    pub fn create_client(bind: Addr, time: u32) -> Self {
        Self::create(bind, Role::Client, time, 3000, 300)
    }

    /// `drdp_create_addr`, without the sockets.
    pub fn create(bind: Addr, role: Role, time: u32, bps: i32, queue: i32) -> Self {
        let mut d = Drdp {
            time,
            time_flush: time,
            time_bps: time,
            bytes_sent: 0,
            bytes_recv: 0,
            bytes_dropped: 0,
            role,
            no_send: false,
            bind,
            endpoints: BTreeMap::new(),
            routes_by_time: Vec::new(),
            routes_by_id: BTreeMap::new(),
            arrivals: Vec::new(),
            leaders: BTreeMap::new(),
            conns_by_dst: Vec::new(),
            route_table: RouteTable::new(SW_ENDPOINT_MIN, SW_ENDPOINT_MAX),
            route_cache: RouteCache::default(),
            bandwidth: Bandwidth::new(bps, queue),
            system_endpoint: 0,
            master_connection: None,
            msgs: Arena::default(),
            eps: Arena::default(),
            conns: Arena::default(),
            routes: Arena::default(),
            outbox: Vec::new(),
        };
        let port = match time as u16 {
            PORT_NONE => 0,
            PORT_LEADER => 1,
            PORT_UNREACHABLE => 2,
            p => p,
        };
        let ep = d.endpoint_create(port as u64 | 0xffff_ffff_ffff_0000);
        d.endpoints.insert(d.eps[ep].id, ep);
        d.system_endpoint = ep;
        d
    }

    pub fn is_client(&self) -> bool {
        self.role == Role::Client
    }

    pub fn port_drdp(&self) -> u16 {
        self.eps[self.system_endpoint].id as u16
    }

    pub fn port_udp(&self) -> u16 {
        self.bind.port
    }

    pub fn set_port_udp(&mut self, port: u16) {
        self.bind.port = port;
    }

    /// `drdp_set_time`.
    pub fn set_time(&mut self, time: u32) {
        if time.wrapping_sub(self.time_bps) as i32 > 1000 {
            self.bytes_sent = 0;
            self.bytes_recv = 0;
            self.bytes_dropped = 0;
            self.time_bps = time;
        }
        self.time = time;
    }

    pub fn bps_sent(&self) -> i32 {
        self.bandwidth.bps_sent
    }

    pub fn bps_recv(&self) -> i32 {
        self.bandwidth.bps_recv
    }

    pub fn flush_interval(&self) -> vi::fpu::F<M> {
        self.bandwidth.process_interval_max()
    }

    // ------------------------------------------------------------ messages

    /// `message_create` (a null payload is zeroed here, uninitialised there).
    #[allow(clippy::too_many_arguments)]
    pub(crate) fn message_create(
        &mut self,
        ep: Option<EpId>,
        conn: Option<ConnId>,
        src: u64,
        dst: u64,
        data: Option<&[u8]>,
        size: usize,
        ty: u8,
        seqnum: u16,
        time: u32,
    ) -> MsgId {
        let data = match data {
            Some(d) => d[..size].to_vec(),
            None => vec![0; size],
        };
        self.msgs.insert(Message { endpoint: ep, connection: conn, src, dst, data, seqnum, ty, created: time, time, ..Default::default() })
    }

    pub(crate) fn message_destroy(&mut self, m: MsgId) {
        self.msgs.remove(m);
    }

    /// `message_set_connection`.
    pub(crate) fn message_set_connection(&mut self, m: MsgId, c: Option<ConnId>) {
        self.msgs[m].connection = c;
        if let Some(c) = c {
            self.msgs[m].endpoint = Some(self.conns[c].endpoint);
        }
    }

    // ------------------------------------------------------------ routes

    /// `drdp_get_route`: the route to `id`, made if there is none.
    pub(crate) fn get_route(&mut self, id: u64) -> RouteId {
        if let Some(&r) = self.routes_by_id.get(&id) {
            return r;
        }
        let r = self.routes.insert(Route::new(id, self.time));
        self.routes_by_id.insert(id, r);
        self.routes_insert(r);
        r
    }

    fn routes_insert(&mut self, r: RouteId) {
        let routes = &self.routes;
        insert_sorted(&mut self.routes_by_time, r, |x| routes[x].desired_flush_time as u64, |a, b| cmp_u32_wrap(a as u32, b as u32));
    }

    /// `drdp_route_destroy`.
    fn route_destroy(&mut self, r: RouteId) {
        let id = self.routes[r].id;
        self.routes_by_id.remove(&id);
        remove(&mut self.routes_by_time, &r);
        let route = self.routes.remove(r).unwrap();
        for m in route.pending {
            self.message_destroy(m);
        }
    }

    /// `drdp_resort` / `routes_by_desired_flush_time_resort`.
    pub(crate) fn drdp_resort(&mut self, r: RouteId) {
        remove(&mut self.routes_by_time, &r);
        self.route_recalc_desired_flush_time(r);
        self.routes_insert(r);
    }

    /// `drdp_get_route_endpoint`: a hardware id is its own route.
    pub fn get_route_endpoint(&self, id: u64) -> u64 {
        let mut route = if is_hardware_endpoint(id) {
            id
        } else {
            match self.route_table.find(id) {
                Some(r) => r,
                None if self.role == Role::Master => u64::MAX,
                None => self.master_connection.map_or(u64::MAX, |c| self.conns[c].remote_id),
            }
        };
        if self.eps[self.system_endpoint].id == route {
            route = self.master_connection.map_or(u64::MAX, |c| self.conns[c].remote_id);
        }
        route
    }

    pub(crate) fn remap_address_to_route(&mut self, a: &Addr) -> Addr {
        let t = self.time;
        self.route_cache.address_to_route(a, t)
    }

    // ------------------------------------------------------------ sending

    /// `drdp_crc_send`: the CRC trailer (XOR'd with the version-2 entry) and
    /// out to the socket.
    pub(crate) fn crc_send(&mut self, to: Addr, mut b: Buffer) {
        let c = CRC_TABLE[2] ^ crc(0, b.written());
        b.put_u32(c);
        self.bytes_sent += b.pos as i32;
        if !self.no_send {
            self.outbox.push(Packet { to, data: b.written().to_vec() });
        }
        let t = self.time;
        self.bandwidth.send(t, b.pos as i32 + 28);
    }

    /// `drdp_send_addr`: connectionless datagrams, drdp port 0xffff, no CRC.
    pub fn send_addr(&mut self, to: &[Addr], data: &[u8]) {
        let mut p = vec![0xff, 0xff];
        p.extend_from_slice(data);
        for a in to {
            self.bytes_sent += data.len() as i32;
            if !self.no_send {
                self.outbox.push(Packet { to: *a, data: p.clone() });
            }
            let t = self.time;
            self.bandwidth.send(t, p.len() as i32 + 28);
        }
    }

    /// `drdp_flush`: flush every route that is due, while the process-wide
    /// bucket allows.
    pub fn flush(&mut self) {
        let t = self.time;
        self.route_cache.release_expired(t);
        loop {
            let t = self.time;
            if (self.bandwidth.flush_time(t).wrapping_sub(t) as i32) > 0 {
                break;
            }
            let Some(&r) = self.routes_by_time.first() else { return };
            if self.time < self.routes[r].desired_flush_time {
                break;
            }
            self.route_flush(r);
            if self.route_destroy_check(r) {
                self.route_destroy(r);
            } else {
                self.drdp_resort(r);
            }
        }
        self.time_flush = self.time;
    }

    /// When the next flush is due: the first route's desired flush time.
    pub fn next_flush_time(&self) -> Option<u32> {
        self.routes_by_time.first().map(|&r| self.routes[r].desired_flush_time)
    }

    // ------------------------------------------------------------ receiving

    /// `drdp_process_packet`: a datagram behind its sender's address (four
    /// octets, the UDP port, then the payload). A zero port is ICMP, which is
    /// not ported.
    pub fn process_packet(&mut self, data: &[u8]) -> i32 {
        let mut b = Buffer::reader(data);
        b.skip(4);
        if b.u16() != 0 { self.process_udp(data) } else { -1 }
    }

    /// `drdp_process_udp`.
    fn process_udp(&mut self, data: &[u8]) -> i32 {
        let t = self.time;
        self.bandwidth.recv(t, data.len() as i32 + 22);
        let mut b = Buffer::reader(data);
        let mut ip = [0u8; 4];
        b.read_into(&mut ip);
        let mut address = Addr { ip, port: b.u16() };
        let mut buffer = Buffer::reader(b.rest());
        let drdp_port = buffer.u16();
        if drdp_port == PORT_NONE {
            return self.process_connectionless(&address, &mut buffer);
        }
        address = self.route_cache.route_to_address(&address, t);
        let route = address_to_endpoint(address.ip, address.port, drdp_port);
        let version = consume_crc_get_protocol_version(&mut buffer);
        if drdp_port == PORT_UNREACHABLE {
            if version == -1 || version == 2 {
                return 0;
            }
            self.route_terminate(&address, 14);
            return 0;
        }
        if version == -1 {
            let Some(&r) = self.routes_by_id.get(&route) else { return -1 };
            for c in self.routes[r].connections.clone() {
                self.conns[c].cstat[5] += 1;
                self.conns[c].cstat[3] += 1;
            }
            return -1;
        }
        if version != 2 {
            self.send_proto_unreachable(&address);
            return -1;
        }
        if buffer.remaining() == 0 {
            if let Some(&r) = self.routes_by_id.get(&route) {
                self.route_terminate_route(r, 15);
            }
            return 0;
        }
        let mut port = buffer.u16();
        if port == PORT_LEADER {
            port = self.port_drdp();
        }
        if port != self.port_drdp() {
            self.send_port_unreachable(&address, port);
            return -1;
        }
        let start = buffer.pos;
        let sys = self.eps[self.system_endpoint].id;
        while buffer.remaining() != 0 {
            let h = read_segment_header(&mut buffer, route, sys);
            buffer.skip(h.size as usize);
            if !buffer.overflow && is_endpoint(h.dst) && is_endpoint(h.src) {
                continue;
            }
            let Some(&r) = self.routes_by_id.get(&route) else { return -1 };
            for c in self.routes[r].connections.clone() {
                self.conns[c].cstat[5] += 1;
                self.conns[c].cstat[2] += 1;
            }
            return -1;
        }
        buffer.pos = start;
        while buffer.remaining() != 0 {
            let r = self.handle_segment(&mut buffer, route);
            if r != 0 {
                return r;
            }
        }
        if let Some(&r) = self.routes_by_id.get(&route) {
            let t = self.time;
            self.routes[r].bandwidth.recv(t, data.len() as i32 - 6 + 28);
        }
        0
    }

    fn process_connectionless(&mut self, a: &Addr, b: &mut Buffer) -> i32 {
        let src = address_to_endpoint(a.ip, a.port, PORT_NONE);
        let rest = b.rest().to_vec();
        let t = self.time;
        let m = self.message_create(None, None, src, u64::MAX, Some(&rest), rest.len(), 248, 0, t);
        self.arrivals.push(m);
        0
    }

    fn send_port_unreachable(&mut self, a: &Addr, port: u16) {
        let route = self.remap_address_to_route(a);
        let mut b = Buffer::new(32);
        b.put_u16(port);
        self.crc_send(route, b);
    }

    fn send_proto_unreachable(&mut self, a: &Addr) {
        let route = self.remap_address_to_route(a);
        let mut b = Buffer::new(32);
        b.put_u16(PORT_UNREACHABLE);
        self.crc_send(route, b);
    }

    /// `drdp_route_terminate`: every route to that address, whatever its
    /// drdp port.
    fn route_terminate(&mut self, a: &Addr, reason: u8) {
        let lo = address_to_endpoint(a.ip, a.port, 0);
        let hi = address_to_endpoint(a.ip, a.port, PORT_NONE);
        let rs: Vec<RouteId> = self.routes_by_id.range(lo..=hi).map(|(_, &r)| r).collect();
        for r in rs {
            self.route_terminate_route(r, reason);
        }
    }

    /// `drdp_handle_segment`.
    fn handle_segment(&mut self, b: &mut Buffer, route: u64) -> i32 {
        let sys = self.eps[self.system_endpoint].id;
        let mut h = read_segment_header(b, route, sys);
        if h.flags & 0x20000 != 0 {
            self.terminate_connections_to_endpoint(h.src);
            return 0;
        }
        let Some(&ep) = self.endpoints.get(&h.dst) else {
            self.handle_segment_endpoint_not_local(&h, b);
            return 0;
        };
        self.endpoint_handle_segment(ep, &mut h, b)
    }

    fn handle_segment_endpoint_not_local(&mut self, h: &SegmentHeader, b: &mut Buffer) {
        if is_local_hardware_endpoint(h.dst) {
            self.send_reset_endpoint(h);
            return;
        }
        let route = self.get_route_endpoint(h.dst);
        b.skip(h.size as usize);
        if route == u64::MAX && is_hardware_endpoint(h.src) && !self.is_leader_endpoint(h.src) {
            self.send_reset_endpoint(h);
        } else if route != u64::MAX {
            self.get_route(route);
        }
    }

    pub fn is_leader_endpoint(&self, id: u64) -> bool {
        matches!(self.leaders.get(&id), Some(Some(_)))
    }

    /// `drdp_terminate_connections_to_endpoint`.
    fn terminate_connections_to_endpoint(&mut self, id: u64) {
        for c in self.connections_to(id) {
            if self.conns.contains(c) {
                let (s, d) = (self.conns[c].local_id, self.conns[c].remote_id);
                if is_hardware_endpoint(s) || is_hardware_endpoint(d) {
                    self.connection_terminate_reason(c, 0, 5);
                }
            }
        }
    }

    /// The connections to `id`, in the drdp tree's order.
    pub(crate) fn connections_to(&self, id: u64) -> Vec<ConnId> {
        self.conns_by_dst.iter().copied().filter(|&c| self.conns[c].remote_id == id).collect()
    }

    /// `drdp_re_route_connections_to_endpoint`.
    pub(crate) fn re_route_connections_to_endpoint(&mut self, id: u64, route: u64) {
        let cs = self.connections_to(id);
        if cs.is_empty() {
            return;
        }
        let r = self.get_route(route);
        for c in cs {
            if self.conns[c].route != Some(r) {
                self.route_link_connection(r, c);
            }
        }
    }

    /// `drdp_close_connections_to_endpoint`.
    fn close_connections_to_endpoint(&mut self, id: u64) {
        for c in self.connections_to(id) {
            if self.conns.contains(c) {
                self.endpoint_connection_destroy(c);
            }
        }
    }

    pub(crate) fn send_reset_connection(&mut self, h: &SegmentHeader) {
        if h.flags & 0x30000 != 0 {
            return;
        }
        let r = self.get_route(h.route);
        self.route_send_reset_connection(r, h.dst, h.src, if h.flags & 0x2000 != 0 { 2 } else { 0 }, 0, h.crc);
    }

    pub(crate) fn send_reset_endpoint(&mut self, h: &SegmentHeader) {
        if h.flags & 0x30000 != 0 {
            return;
        }
        let r = self.get_route(h.route);
        self.routes[r].reset_endpoints.insert(h.dst);
        self.drdp_resort(r);
    }

    /// `drdp_receive`: the next arrived message, taken off every list. A
    /// system message is acted on here first and still returned.
    pub fn receive(&mut self) -> Option<Message> {
        let &m = self.arrivals.first()?;
        match self.msgs[m].endpoint {
            None => {
                remove(&mut self.arrivals, &m);
            }
            Some(ep) => self.endpoint_arrival_remove(ep, m),
        }
        if self.msgs[m].ty == TYPE_SYSTEM {
            self.process_system_message(m);
        }
        let (s, d) = (self.msgs[m].src, self.msgs[m].dst);
        if is_software_endpoint(s) && is_software_endpoint(d) {
            self.message_set_connection(m, None);
        }
        self.msgs.remove(m)
    }

    /// `connection_terminate_file_line` with drdp.cpp's name.
    fn terminate_at(&mut self, c: Option<ConnId>, line: u32) {
        let Some(c) = c else { return };
        if self.conns[c].flags & 0x100 != 0 {
            return;
        }
        let mut b = Buffer::new(256);
        b.put_u8(16);
        b.put_string(DRDP_CPP);
        b.put_u32(line);
        let data = b.written().to_vec();
        self.connection_terminate(c, 1, &data);
    }

    /// `drdp_process_system_message`, the cases a client can reach.
    fn process_system_message(&mut self, m: MsgId) {
        let (src, conn) = (self.msgs[m].src, self.msgs[m].connection);
        let mut b = Buffer::reader(&self.msgs[m].data);
        match b.u8() {
            // The keep-alive, and the leader-only messages whose client
            // branch does nothing (the original asserts on the role).
            18 | 1 | 3 | 6 => {}
            7 => self.terminate_at(conn, 2764),
            5 => {
                if !self.leaders.contains_key(&src) {
                    self.terminate_at(conn, 2853);
                }
            }
            36 => {
                if Some(src) == self.master_connection.map(|c| self.conns[c].remote_id) {
                    let id = b.u64();
                    self.route_table.update_block(id, src);
                } else {
                    self.terminate_at(conn, 2971);
                }
            }
            20 => {
                if Some(src) == self.master_connection.map(|c| self.conns[c].remote_id) {
                    let id = b.u64();
                    self.handle_endpoint_info(id, src);
                } else {
                    self.terminate_at(conn, 3104);
                }
            }
            21 => {
                let id = b.u64();
                self.route_table.update(id, u64::MAX);
                self.close_connections_to_endpoint(id);
            }
            19 => {
                let (id, route) = (b.u64(), b.u64());
                self.handle_endpoint_info(id, route);
            }
            35 => {
                let (a, r) = (b.u64(), b.u64());
                self.route_table.update_block(a, r);
            }
            // 2, 17, 32-34 and 48 are leader/master traffic over a master
            // connection a client never has (see the crate docs).
            _ => {}
        }
    }

    fn handle_endpoint_info(&mut self, id: u64, route: u64) {
        self.route_table.update(id, route);
        if route == u64::MAX {
            self.close_connections_to_endpoint(id);
        } else {
            self.re_route_connections_to_endpoint(id, route);
        }
    }
}

/// `drdp_consume_crc_get_protocol_version`: strips the trailer and returns
/// the version it names, or -1 for a bad CRC.
fn consume_crc_get_protocol_version(b: &mut Buffer) -> i32 {
    if b.max < 4 {
        return -1;
    }
    let pos = b.pos;
    b.pos = b.max - 4;
    let c = b.u32();
    b.pos = pos;
    b.max -= 4;
    crc_table_lookup(c ^ crc(0, &b.data[..b.max]), 2)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn crc_table_is_the_decomps() {
        assert_eq!(CRC_TABLE[1], 0x77073096);
        assert_eq!(CRC_TABLE[2], 0xee0e612c);
        assert_eq!(CRC_TABLE[255], 0x2d02ef8d);
    }

    #[test]
    fn endpoint_codec_round_trips() {
        let id = address_to_endpoint([10, 1, 2, 3], 10070, 0xfffe);
        assert_eq!(id, 0x0a01_0203_2756_fffe);
        assert_eq!(endpoint_to_address(id), (Addr { ip: [10, 1, 2, 3], port: 10070 }, 0xfffe));
        assert!(is_remote_hardware_endpoint(id));
        assert!(is_local_hardware_endpoint(0xffff_ffff_ffff_1234));
    }
}
