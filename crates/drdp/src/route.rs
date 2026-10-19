//! `route.cpp`: one UDP peer. Every connection to that machine hangs off it,
//! sorted by when each wants to go out next, and `route_flush` is the only
//! place a packet is built.

use crate::bandwidth::Bandwidth;
use crate::buffer::Buffer;
use crate::list::{cmp_u32_wrap, insert_sorted, remove};
use crate::{ConnId, Drdp, MsgId, RouteId, SegmentHeader, append_segment_header, endpoint_to_address, is_hardware_endpoint};
use std::collections::BTreeSet;
use vi::fpu::Model;

const ONE_DAY: u32 = 86400000;

/// `reset_connection_t`; the derived order is `reset_connection_cmp`'s.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub struct ResetConnection {
    pub src: u64,
    pub dst: u64,
    pub flags: i32,
    pub instance_local: u32,
    pub instance_remote: u32,
}

#[derive(Debug)]
pub struct Route<M: Model> {
    pub id: u64,
    pub desired_flush_time: u32,
    /// Raw segments waiting for a packet (forwarded ones).
    pub pending: Vec<MsgId>,
    /// By desired flush time (wrapping), insert after equals.
    pub connections: Vec<ConnId>,
    pub reset_connections: BTreeSet<ResetConnection>,
    pub reset_endpoints: BTreeSet<u64>,
    pub bandwidth: Bandwidth<M>,
    pub terminated: bool,
    pub drop: i32,
    pub delay: i32,
    pub jitter: i32,
    pub flush_number: i32,
}

impl<M: Model> Route<M> {
    /// `route_create`: 3,000 bps, a 300-byte queue.
    pub fn new(id: u64, time: u32) -> Self {
        Route {
            id,
            desired_flush_time: time,
            pending: Vec::new(),
            connections: Vec::new(),
            reset_connections: BTreeSet::new(),
            reset_endpoints: BTreeSet::new(),
            bandwidth: Bandwidth::new(3000, 300),
            terminated: false,
            drop: 0,
            delay: 0,
            jitter: 0,
            flush_number: 0,
        }
    }
}

impl<M: Model> Drdp<M> {
    /// `route_destroy_check`: nothing left for it to send.
    pub(crate) fn route_destroy_check(&self, r: RouteId) -> bool {
        let route = &self.routes[r];
        route.connections.is_empty() && route.reset_connections.is_empty() && route.reset_endpoints.is_empty() && route.pending.is_empty()
    }

    fn route_insert_connection(&mut self, r: RouteId, c: ConnId) {
        let conns = &self.conns;
        insert_sorted(&mut self.routes[r].connections, c, |x| conns[x].desired_flush_time as u64, |a, b| cmp_u32_wrap(a as u32, b as u32));
    }

    /// `route_unlink_connection`.
    pub(crate) fn route_unlink_connection(&mut self, c: ConnId) {
        if let Some(r) = self.conns[c].route {
            remove(&mut self.routes[r].connections, &c);
            self.conns[c].route = None;
        }
    }

    /// `route_link_connection`.
    pub(crate) fn route_link_connection(&mut self, r: RouteId, c: ConnId) {
        self.route_unlink_connection(c);
        self.route_insert_connection(r, c);
        self.conns[c].route = Some(r);
    }

    /// `route_terminate`.
    pub(crate) fn route_terminate_route(&mut self, r: RouteId, reason: u8) {
        for c in self.routes[r].connections.clone() {
            if !self.conns.contains(c) {
                continue;
            }
            let (s, d) = (self.conns[c].local_id, self.conns[c].remote_id);
            if is_hardware_endpoint(s) || is_hardware_endpoint(d) {
                self.connection_terminate_reason(c, 0, reason);
            }
        }
        self.routes[r].terminated = true;
    }

    /// `connections_by_desired_flush_time_resort`.
    pub(crate) fn connections_resort(&mut self, r: RouteId, c: ConnId) {
        remove(&mut self.routes[r].connections, &c);
        self.connection_recalc_desired_flush_time(c);
        self.route_insert_connection(r, c);
    }

    /// `route_resort`.
    pub(crate) fn route_resort(&mut self, c: ConnId) {
        let Some(r) = self.conns[c].route else { return };
        self.connections_resort(r, c);
        self.drdp_resort(r);
    }

    /// `route_send_reset_connection`.
    pub(crate) fn route_send_reset_connection(&mut self, r: RouteId, src: u64, dst: u64, flags: i32, local: u32, remote: u32) {
        let reset = ResetConnection {
            src,
            dst,
            flags,
            instance_local: if flags & 1 != 0 { local } else { 0 },
            instance_remote: if flags & 2 != 0 { remote } else { 0 },
        };
        self.routes[r].reset_connections.insert(reset);
        self.drdp_resort(r);
    }

    /// `route_recalc_desired_flush_time`.
    pub(crate) fn route_recalc_desired_flush_time(&mut self, r: RouteId) {
        let t = self.time;
        let first = self.routes[r].connections.first().copied();
        let route = &mut self.routes[r];
        route.desired_flush_time = match first {
            Some(c) if !route.terminated => self.conns[c].desired_flush_time,
            _ => t.wrapping_add(ONE_DAY),
        };
        if (!route.reset_connections.is_empty() || !route.reset_endpoints.is_empty()) && route.desired_flush_time.wrapping_sub(t) as i32 >= 0 {
            route.desired_flush_time = t;
        }
        let when = route.bandwidth.flush_time(t);
        if when.wrapping_sub(route.desired_flush_time) as i32 > 0 {
            route.desired_flush_time = when;
        }
    }

    /// `route_flush`: one packet to this route's peer. The two drdp ports,
    /// then queued resets, forwarded segments, and a segment for each due
    /// connection while room is left.
    pub(crate) fn route_flush(&mut self, r: RouteId) {
        let mut b = Buffer::new(1500);
        let (address, port) = endpoint_to_address(self.routes[r].id);
        let to = self.remap_address_to_route(&address);
        b.put_u16(self.port_drdp());
        b.put_u16(port);
        let start = b.pos;
        // route_flush_reset_connection
        while b.remaining() >= 64 {
            let Some(reset) = self.routes[r].reset_connections.pop_first() else { break };
            let mut h = SegmentHeader { flags: 0x10000, src: reset.src, dst: reset.dst, ..Default::default() };
            if reset.flags & 1 != 0 {
                h.flags |= 0x2000;
                h.crc = reset.instance_local;
            }
            if reset.flags & 2 != 0 {
                h.flags |= 0x40000;
                h.flush_number = reset.instance_remote;
            }
            append_segment_header(&mut b, &mut h);
        }
        // route_flush_reset_endpoint
        while b.remaining() >= 64 {
            let Some(id) = self.routes[r].reset_endpoints.pop_first() else { break };
            let mut h = SegmentHeader { flags: 0x20000, src: id, ..Default::default() };
            append_segment_header(&mut b, &mut h);
        }
        while let Some(&m) = self.routes[r].pending.first() {
            if self.msgs[m].data.len() >= b.remaining() as u32 as usize {
                break;
            }
            let data = std::mem::take(&mut self.msgs[m].data);
            b.put_bytes(&data);
            self.routes[r].pending.remove(0);
            self.message_destroy(m);
        }
        while let Some(&c) = self.routes[r].connections.first() {
            let flush_number = self.routes[r].flush_number;
            if self.conns[c].desired_flush_time.wrapping_sub(self.time) as i32 > 0
                || b.remaining() < 128
                || self.conns[c].last_route_flush_number == flush_number
            {
                break;
            }
            let mut cb = Buffer::new((b.remaining() - 64) as usize);
            let result = self.connection_flush(c, &mut cb, flush_number);
            if result == -2 {
                break;
            }
            if result < -1 {
                continue;
            }
            match result {
                0 => {
                    let conn = &self.conns[c];
                    let mut h = SegmentHeader { size: cb.pos as u32, src: conn.local_id, dst: conn.remote_id, ..Default::default() };
                    if self.connection_send_instance_local(c) {
                        h.flags |= 0x2000;
                        if self.conns[c].flags & 0x40 == 0 {
                            h.flags |= 0x80000;
                        }
                        h.crc = self.conns[c].instance_local;
                        if self.conns[c].flags & 0x80 != 0 {
                            h.flags |= 0x4000;
                        }
                    }
                    append_segment_header(&mut b, &mut h);
                    b.put_bytes(cb.written());
                    self.connections_resort(r, c);
                }
                -1 => self.connections_resort(r, c),
                _ => {}
            }
        }
        if start < b.pos {
            self.crc_send(to, b.clone());
            let t = self.time;
            self.routes[r].bandwidth.send(t, b.pos as i32 + 4 + 28);
        }
        let route = &mut self.routes[r];
        route.flush_number = route.flush_number.wrapping_add(1);
        if route.flush_number == -1 {
            route.flush_number = 0;
        }
    }
}
