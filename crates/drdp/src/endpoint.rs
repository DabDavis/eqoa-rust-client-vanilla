//! `endpoint.cpp`: one addressable party, the connections open to it and the
//! messages that have arrived for them.

use crate::buffer::Buffer;
use crate::list::{insert_sorted, remove};
use crate::{CloseSet, ConnId, Drdp, EpId, MsgId, SegmentHeader, address_to_endpoint, endpoint_to_address, is_hardware_endpoint, is_remote_hardware_endpoint, is_software_endpoint, PORT_LEADER};
use std::collections::BTreeMap;
use vi::fpu::Model;

#[derive(Debug, Default)]
pub struct Endpoint {
    pub id: u64,
    /// Seeded from the clock, then handed out one at a time.
    pub instance: u32,
    /// Bit 0 closed, bit 1 in sync (sticky).
    pub flags: i32,
    pub connections: BTreeMap<u64, ConnId>,
    pub arrivals: Vec<MsgId>,
    pub close_set: CloseSet,
    pub a8: u32,
    pub ac: u32,
}

impl<M: Model> Drdp<M> {
    /// `endpoint_create`.
    pub(crate) fn endpoint_create(&mut self, id: u64) -> EpId {
        let t = self.time;
        self.eps.insert(Endpoint { id, instance: t, ac: t, ..Default::default() })
    }

    pub fn endpoint_is_closed(&self, ep: EpId) -> bool {
        self.eps[ep].flags & 1 != 0
    }

    /// `endpoint_get_instance`: post-increment.
    pub(crate) fn endpoint_get_instance(&mut self, ep: EpId) -> u32 {
        let e = &mut self.eps[ep];
        let i = e.instance;
        e.instance = i.wrapping_add(1);
        i
    }

    /// `endpoint_arrival_enqueue`: dropped if the endpoint or its connection
    /// is closed.
    pub(crate) fn endpoint_arrival_enqueue(&mut self, ep: EpId, m: MsgId) {
        let closed = self.endpoint_is_closed(ep) || self.msgs[m].connection.is_some_and(|c| self.connection_is_closed(c));
        if closed {
            self.message_destroy(m);
        } else {
            self.eps[ep].arrivals.push(m);
            self.arrivals.push(m);
        }
    }

    pub(crate) fn endpoint_arrival_remove(&mut self, ep: EpId, m: MsgId) {
        remove(&mut self.arrivals, &m);
        remove(&mut self.eps[ep].arrivals, &m);
    }

    fn endpoint_connection_insert(&mut self, ep: EpId, c: ConnId) {
        let dst = self.conns[c].remote_id;
        self.eps[ep].connections.insert(dst, c);
        let conns = &self.conns;
        insert_sorted(&mut self.conns_by_dst, c, |x| conns[x].remote_id, |a, b| a.cmp(&b));
    }

    fn endpoint_connection_remove(&mut self, c: ConnId) {
        let (ep, dst) = (self.conns[c].endpoint, self.conns[c].remote_id);
        if self.eps[ep].connections.get(&dst) == Some(&c) {
            self.eps[ep].connections.remove(&dst);
        }
        remove(&mut self.conns_by_dst, &c);
    }

    /// `endpoint_connection_create`. `Err` where the original returns
    /// non-zero.
    pub(crate) fn endpoint_connection_create(&mut self, ep: EpId, dst: u64, route: u64, instance: u32, flags: i32) -> Result<ConnId, i32> {
        if route == u64::MAX || self.eps[ep].connections.contains_key(&dst) {
            return Err(-1);
        }
        let r = self.get_route(route);
        let mut cflags = flags & 1;
        if flags & 2 != 0 {
            cflags |= 2;
        }
        let c = self.connection_create(ep, dst, instance, cflags);
        self.endpoint_connection_insert(ep, c);
        self.route_link_connection(r, c);
        Ok(c)
    }

    /// `endpoint_get_connection`: an open one, or a new one between two
    /// software endpoints.
    fn endpoint_get_connection(&mut self, ep: EpId, dst: u64) -> Result<ConnId, i32> {
        if let Some(&c) = self.eps[ep].connections.get(&dst) {
            return Ok(c);
        }
        if !is_software_endpoint(dst) || !is_software_endpoint(self.eps[ep].id) {
            return Err(-1);
        }
        let route = self.get_route_endpoint(dst);
        let inst = self.endpoint_get_instance(ep);
        self.endpoint_connection_create(ep, dst, route, inst, 0)
    }

    /// `endpoint_send`.
    pub fn endpoint_send(&mut self, ep: EpId, dst: u64, data: &[u8], ty: u8) {
        if !self.is_client() && self.get_route_endpoint(dst) == u64::MAX {
            let src = self.eps[ep].id;
            let t = self.time;
            let m = self.message_create(Some(ep), None, dst, src, Some(&[22]), 1, crate::TYPE_SYSTEM, 0, t);
            self.endpoint_arrival_enqueue(ep, m);
            return;
        }
        match self.endpoints.get(&dst).copied() {
            None => {
                if let Ok(c) = self.endpoint_get_connection(ep, dst) {
                    self.connection_send_internal(c, data, ty);
                }
            }
            Some(to) => {
                let src = self.eps[ep].id;
                let t = self.time;
                let m = self.message_create(Some(to), None, src, dst, Some(data), data.len(), ty, 0, t);
                self.endpoint_arrival_enqueue(to, m);
            }
        }
    }

    /// `endpoint_send_reset_connection`.
    fn endpoint_send_reset_connection(&mut self, h: &SegmentHeader) {
        if is_hardware_endpoint(h.src) || is_hardware_endpoint(h.dst) {
            self.send_reset_connection(h);
        }
    }

    /// `endpoint_handle_segment`.
    pub(crate) fn endpoint_handle_segment(&mut self, ep: EpId, h: &mut SegmentHeader, b: &mut Buffer) -> i32 {
        if h.flags & 0x2000 != 0 && self.eps[ep].close_set.lookup(h.src, h.crc) {
            self.endpoint_send_reset_connection(h);
            b.skip(h.size as usize);
            return 0;
        }
        let t = self.time;
        self.eps[ep].close_set.release_expired(t);
        let mut conn = self.eps[ep].connections.get(&h.src).copied();
        if conn.is_none() && is_remote_hardware_endpoint(h.src) {
            let (addr, _) = endpoint_to_address(h.src);
            let leader = address_to_endpoint(addr.ip, addr.port, PORT_LEADER);
            if let Some(&c) = self.eps[ep].connections.get(&leader) {
                let route = self.get_route_endpoint(h.src);
                let r = self.get_route(route);
                let limit = self.routes[self.conns[c].route.unwrap_or(r)].bandwidth.bps_limit.0 as i32;
                self.endpoint_connection_remove(c);
                self.route_unlink_connection(c);
                self.conns[c].remote_id = h.src;
                self.route_link_connection(r, c);
                self.endpoint_connection_insert(ep, c);
                self.connection_set_bps_limit(c, limit);
                conn = Some(c);
            }
        }
        if let Some(c) = conn {
            if h.flags & 0x10000 != 0 {
                return self.connection_handle_reset(c, h);
            }
            if h.flags & 0x2000 != 0 && self.conns[c].flags & 0x20 != 0 && h.crc != self.conns[c].instance_remote {
                if is_hardware_endpoint(h.src) || is_hardware_endpoint(h.dst) {
                    if h.flags & 0x4000 != 0 {
                        self.connection_terminate_reason(c, 1, 11);
                    }
                } else {
                    self.connection_rx_reset(c);
                }
            }
        }
        let mut created = false;
        if conn.is_none() {
            if h.flags & 0x2000 != 0 && h.flags & 0x10000 == 0 {
                let route = if self.is_client() { h.route } else { self.get_route_endpoint(h.src) };
                let (instance, flags) = if h.flags & 0x4000 != 0 { (h.crc, 2) } else { (self.endpoint_get_instance(ep), 0) };
                created = true;
                match self.endpoint_connection_create(ep, h.src, route, instance, flags) {
                    Ok(c) => conn = Some(c),
                    Err(e) => return e,
                }
            } else {
                self.endpoint_send_reset_connection(h);
            }
        }
        if let Some(c) = conn {
            if self.connection_is_terminated(c) {
                self.connection_send_reset(c);
            } else {
                let mut seg = Buffer::reader(&b.rest()[..(h.size as usize).min(b.rest().len())]);
                seg.max = h.size as usize;
                if seg.data.len() < seg.max {
                    seg.data.resize(seg.max, 0);
                }
                let r = self.connection_handle_segment(c, h, &mut seg);
                if r != 0 {
                    if created {
                        self.endpoint_connection_destroy(c);
                    }
                    return r;
                }
            }
        }
        b.skip(h.size as usize);
        0
    }

    /// `endpoint_connection_free_arrivals`.
    pub(crate) fn endpoint_connection_free_arrivals(&mut self, c: ConnId) {
        let ep = self.conns[c].endpoint;
        for m in self.eps[ep].arrivals.clone() {
            if self.msgs[m].connection == Some(c) {
                if is_software_endpoint(self.msgs[m].src) && is_software_endpoint(self.msgs[m].dst) {
                    self.message_set_connection(m, None);
                } else {
                    self.endpoint_arrival_remove(ep, m);
                    self.message_destroy(m);
                }
            }
        }
    }

    /// `_endpoint_connection_destroy`: off every container, its arrivals
    /// freed unless already closed, and its remote instance remembered.
    pub(crate) fn endpoint_connection_destroy(&mut self, c: ConnId) {
        let ep = self.conns[c].endpoint;
        self.endpoint_connection_remove(c);
        self.route_unlink_connection(c);
        if !self.connection_is_closed(c) {
            self.endpoint_connection_free_arrivals(c);
        }
        if self.conns[c].flags & 0x20 != 0 {
            let (dst, inst, last) = (self.conns[c].remote_id, self.conns[c].instance_remote, self.conns[c].time_last_instance_arrival);
            let t = self.time;
            self.eps[ep].close_set.insert(dst, inst, last, t);
        }
        self.connection_destroy(c);
    }

    /// `endpoint_is_in_sync`: sticky once every connection is.
    pub(crate) fn endpoint_is_in_sync(&mut self, ep: EpId) -> bool {
        if self.eps[ep].flags & 2 == 0 {
            let cs: Vec<ConnId> = self.eps[ep].connections.values().copied().collect();
            if cs.into_iter().all(|c| self.connection_is_in_sync(c)) {
                self.eps[ep].flags |= 2;
            }
        }
        self.eps[ep].flags & 2 != 0
    }

    /// `endpoint_find_connection`: an open connection to `dst` (none between
    /// two software endpoints, none once closed).
    pub fn endpoint_find_connection(&self, ep: EpId, dst: u64) -> Option<ConnId> {
        let c = *self.eps[ep].connections.get(&dst)?;
        if is_software_endpoint(self.eps[ep].id) && is_software_endpoint(dst) {
            return None;
        }
        if self.connection_is_closed(c) { None } else { Some(c) }
    }

    /// `drdp_endpoint_destroy`, for a client: the endpoint and its
    /// connections go.
    pub(crate) fn drdp_endpoint_destroy(&mut self, ep: EpId) {
        let id = self.eps[ep].id;
        self.route_table.update(id, u64::MAX);
        self.endpoints.remove(&id);
        let cs: Vec<ConnId> = self.eps[ep].connections.values().copied().collect();
        for c in cs {
            self.endpoint_connection_remove(c);
            self.route_unlink_connection(c);
            self.connection_destroy(c);
        }
        if !self.endpoint_is_closed(ep) {
            for m in self.eps[ep].arrivals.clone() {
                self.endpoint_arrival_remove(ep, m);
                self.message_destroy(m);
            }
        }
        self.eps.remove(ep);
    }
}
