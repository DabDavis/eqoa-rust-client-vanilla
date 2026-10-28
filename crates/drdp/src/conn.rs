//! `connection.cpp`: one conversation between two endpoints. Three ways to
//! send (best effort, guaranteed in 1,300-byte fragments, and 248 state
//! channels that send only what changed, XOR'd against an acknowledged
//! version and run-length coded), the flush that writes them into a segment
//! with the acks, and the parser and bookkeeping on the way in.

use crate::buffer::{Buffer, rle_decode, rle_encode};
use crate::list::{cmp_u16_wrap, insert_sorted, lookup_sorted, remove};
use crate::{
    ConnId, Drdp, EpId, MsgId, RouteId, SegmentHeader, TYPE_BEST_EFFORT, TYPE_DISCONNECT, TYPE_FRAGMENT, TYPE_GUARANTEED, TYPE_STATE_CHANNELS, TYPE_SYSTEM,
    is_hardware_endpoint, is_software_endpoint,
};
use vi::fpu::Model;

const CHANNELS: usize = 248;
const IDLE_RESET: u32 = 120000;
const SEND_SIZE_MAX_GUARANTEED: usize = 0x514000;
const ONE_DAY: u32 = 86400000;

fn u16_gt(a: u16, b: u16) -> bool {
    (a.wrapping_sub(b) as i16) > 0
}
fn u16_lt(a: u16, b: u16) -> bool {
    (a.wrapping_sub(b) as i16) < 0
}
fn u16_le(a: u16, b: u16) -> bool {
    (a.wrapping_sub(b) as i16) <= 0
}
fn u16_ge(a: u16, b: u16) -> bool {
    (a.wrapping_sub(b) as i16) >= 0
}
fn u16_diff(a: u16, b: u16) -> i32 {
    a.wrapping_sub(b) as i16 as i32
}
fn u32_ge(a: u32, b: u32) -> bool {
    (a.wrapping_sub(b) as i32) >= 0
}
fn u32_gt(a: u32, b: u32) -> bool {
    (a.wrapping_sub(b) as i32) > 0
}

/// `state_channel_t`.
#[derive(Clone, Debug)]
pub struct StateChannel {
    pub pending: Option<MsgId>,
    pub tx: u16,
    pub tx_ack: u16,
    pub rx: u16,
    pub rx_flag: u8,
    /// 0 nowhere, 1 send queue, 2 outstanding.
    pub where_: u8,
    /// Sent, by sequence number.
    pub tx_list: Vec<MsgId>,
    /// Arrived, by sequence number.
    pub rx_list: Vec<MsgId>,
}

impl Default for StateChannel {
    fn default() -> Self {
        StateChannel { pending: None, tx: 1, tx_ack: 0, rx: 0, rx_flag: 0, where_: 0, tx_list: Vec::new(), rx_list: Vec::new() }
    }
}

/// `connection_t`.
#[derive(Debug)]
pub struct Connection {
    pub endpoint: EpId,
    pub route: Option<RouteId>,
    pub local_id: u64,
    pub remote_id: u64,
    pub instance_local: u32,
    pub instance_remote: u32,
    pub time_instance_sent: u32,
    pub time_last_flush: u32,
    pub time_last_arrival: u32,
    pub time_last_instance_arrival: u32,
    pub time_send: u32,
    pub desired_flush_time: u32,
    pub flush_by: u32,
    pub send_queue: Vec<MsgId>,
    pub outstanding: Vec<MsgId>,
    /// By sequence number (wrapping).
    pub arrivals: Vec<MsgId>,
    pub delayed: Vec<MsgId>,
    pub terminate: Option<MsgId>,
    pub flush_time: Box<[u32; 1024]>,
    pub flags: u16,
    pub rtt_index: u8,
    pub rx_flag: u8,
    pub timeout: u16,
    pub rtt: [u16; 16],
    pub flush: u16,
    pub arrival_number: u16,
    pub flush_ack: u16,
    pub rx_mask: u64,
    pub arrival_history: u64,
    pub tx_seqnum: u16,
    pub rx_seqnum: u16,
    pub tx_ack: u16,
    pub tx_mask: u64,
    pub retry_time: i32,
    pub warn_time: i32,
    pub fail_time: i32,
    pub last_route_flush_number: i32,
    pub cstat: [u64; 11],
    pub channels: Option<Vec<StateChannel>>,
}

/// `connection_get_cstat_label`: the eleven counters' names, in the order
/// `cstat` keeps them (`connection_get_cstat_max` is their count). CLIENT's
/// `VINetStats` writes them out for 0x07E1.
pub const CSTAT_LABELS: [&str; 11] = [
    "discarded (old)",
    "discarded (no memory was available to process message)",
    "discarded (corrupt)",
    "discarded (crc)",
    "out of sequence",
    "recv",
    "sent",
    "guaranteed recv (duplicates)",
    "guaranteed recv",
    "state recv (duplicates)",
    "state recv",
];

/// `segment_body_t`.
#[derive(Debug, Default)]
struct Body {
    flags: u8,
    conn_flags: u16,
    channels: Vec<u8>,
    instance: u32,
    arrival: bool,
    guaranteed_ack: bool,
    acks: bool,
    arrival_number: u16,
    flush: u16,
    flush_mask: u64,
    seqnum: u16,
    mask: u64,
    /// (channel, seqnum), ended by channel 248.
    state_acks: Vec<(u8, u16)>,
    messages: Vec<MsgId>,
}

impl<M: Model> Drdp<M> {
    fn msg_seq(&self, m: MsgId) -> u64 {
        self.msgs[m].seqnum as u64
    }

    /// `message_seqnum_remove_thru`: drops the head while it is before
    /// `seqnum`.
    fn seqnum_remove_thru(&mut self, list: &mut Vec<MsgId>, seqnum: u16) {
        while let Some(&m) = list.first() {
            if (self.msgs[m].seqnum.wrapping_sub(seqnum) as i16) >= 0 {
                break;
            }
            list.remove(0);
            self.message_destroy(m);
        }
    }

    fn insert_by_seqnum(&self, list: &mut Vec<MsgId>, m: MsgId) {
        insert_sorted(list, m, |x| self.msg_seq(x), |a, b| cmp_u16_wrap(a as u16, b as u16));
    }

    /// `connection_create` (with `connection_init`).
    pub(crate) fn connection_create(&mut self, ep: EpId, dst: u64, instance: u32, flags: i32) -> ConnId {
        let t = self.time;
        let local = self.eps[ep].id;
        let mut c = Connection {
            endpoint: ep,
            route: None,
            local_id: local,
            remote_id: dst,
            instance_local: instance,
            instance_remote: 0,
            time_instance_sent: 0,
            time_last_flush: 0,
            time_last_arrival: t,
            time_last_instance_arrival: t,
            time_send: t,
            desired_flush_time: t,
            flush_by: 0,
            send_queue: Vec::new(),
            outstanding: Vec::new(),
            arrivals: Vec::new(),
            delayed: Vec::new(),
            terminate: None,
            flush_time: Box::new([0; 1024]),
            flags: 0,
            rtt_index: 0,
            rx_flag: 0,
            timeout: 1000,
            rtt: [0; 16],
            flush: 1,
            arrival_number: 0,
            flush_ack: 0,
            rx_mask: 0,
            arrival_history: u64::MAX,
            tx_seqnum: 1,
            rx_seqnum: 0,
            tx_ack: 0,
            tx_mask: 0,
            retry_time: 5000,
            warn_time: 30000,
            fail_time: 60000,
            last_route_flush_number: -1,
            cstat: [0; 11],
            channels: None,
        };
        if flags & 1 != 0 {
            c.instance_remote = c.instance_local;
            c.flags |= 0xa0;
        }
        if flags & 2 != 0 {
            c.flags |= 0x40;
        }
        let sw_sw = is_software_endpoint(local) && is_software_endpoint(dst);
        let id = self.conns.insert(c);
        if !sw_sw {
            let m = self.message_create(Some(ep), Some(id), dst, local, None, 256, TYPE_DISCONNECT, 0xffff, t);
            self.conns[id].terminate = Some(m);
        }
        id
    }

    fn connection_free_outstanding(&mut self, c: ConnId) {
        let conn = &mut self.conns[c];
        let mut all: Vec<MsgId> = Vec::new();
        all.append(&mut conn.send_queue);
        all.append(&mut conn.outstanding);
        all.append(&mut conn.delayed);
        all.append(&mut conn.arrivals);
        if let Some(chs) = &mut conn.channels {
            for ch in chs.iter_mut() {
                all.append(&mut ch.tx_list);
                all.append(&mut ch.rx_list);
            }
        }
        for m in all {
            self.message_destroy(m);
        }
    }

    /// `connection_destroy`: frees it and everything it holds.
    pub(crate) fn connection_destroy(&mut self, c: ConnId) {
        let Some(conn) = self.conns.remove(c) else { return };
        let mut all: Vec<MsgId> = Vec::new();
        all.extend(conn.send_queue);
        all.extend(conn.outstanding);
        all.extend(conn.arrivals);
        all.extend(conn.delayed);
        if let Some(chs) = conn.channels {
            for ch in chs {
                all.extend(ch.tx_list);
                all.extend(ch.rx_list);
            }
        }
        all.extend(conn.terminate);
        for m in all {
            self.message_destroy(m);
        }
    }

    pub fn connection_is_closed(&self, c: ConnId) -> bool {
        self.conns[c].flags & 0x800 != 0
    }

    pub fn connection_is_terminated(&self, c: ConnId) -> bool {
        self.conns[c].flags & 0x100 != 0
    }

    fn connection_is_sw_sw(&self, c: ConnId) -> bool {
        is_software_endpoint(self.conns[c].local_id) && is_software_endpoint(self.conns[c].remote_id)
    }

    fn allocate_state_channels(&mut self, c: ConnId) {
        let conn = &mut self.conns[c];
        if conn.channels.is_none() {
            conn.channels = Some(vec![StateChannel::default(); CHANNELS]);
        }
    }

    fn send_best_effort(&mut self, c: ConnId, data: &[u8]) -> i32 {
        let (ep, l, r, t) = (self.conns[c].endpoint, self.conns[c].local_id, self.conns[c].remote_id, self.time);
        let m = self.message_create(Some(ep), Some(c), l, r, Some(data), data.len(), TYPE_BEST_EFFORT, 0xffff, t);
        self.conns[c].send_queue.push(m);
        0
    }

    /// `connection_send_guaranteed`: 1,300-byte fragments, every one but the
    /// last typed FRAGMENT; refused past 4,096 unacknowledged.
    fn send_guaranteed(&mut self, c: ConnId, data: &[u8], ty: u8) -> i32 {
        let t = self.time;
        self.conns[c].time_send = t;
        let mut made = Vec::new();
        let size = data.len() as u32;
        let mut off = 0u32;
        while off == 0 || off < size {
            let chunk = (size - off).min(1300);
            let mut kind = ty;
            if off + chunk < size {
                kind = TYPE_FRAGMENT;
            }
            let conn = &self.conns[c];
            if u16_gt(conn.tx_seqnum, conn.tx_ack.wrapping_add(4096)) {
                for m in made {
                    self.message_destroy(m);
                }
                return -1;
            }
            let (ep, l, r, seq) = (conn.endpoint, conn.local_id, conn.remote_id, conn.tx_seqnum);
            self.conns[c].tx_seqnum = seq.wrapping_add(1);
            let piece = &data[off as usize..(off + chunk) as usize];
            made.push(self.message_create(Some(ep), Some(c), l, r, Some(piece), piece.len(), kind, seq, t));
            off += 1300;
        }
        self.conns[c].send_queue.extend(made);
        0
    }

    /// `connection_send_state`: nothing if the bytes equal what is pending
    /// (or last sent); otherwise the new message replaces the pending one
    /// wherever it is.
    fn send_state(&mut self, c: ConnId, data: &[u8], channel: usize) -> i32 {
        if channel >= CHANNELS {
            return -1;
        }
        self.allocate_state_channels(c);
        let ch = &self.conns[c].channels.as_ref().unwrap()[channel];
        let current = ch.pending.or_else(|| ch.tx_list.last().copied());
        let same = match current {
            None => data.is_empty(),
            Some(m) => self.msgs[m].data == data,
        };
        if same {
            return 0;
        }
        let (ep, l, r, t) = (self.conns[c].endpoint, self.conns[c].local_id, self.conns[c].remote_id, self.time);
        let m = self.message_create(Some(ep), Some(c), l, r, Some(data), data.len(), channel as u8, 0xffff, t);
        let (where_, pending) = {
            let ch = &self.conns[c].channels.as_ref().unwrap()[channel];
            (ch.where_, ch.pending)
        };
        let conn = &mut self.conns[c];
        match where_ {
            0 => conn.send_queue.push(m),
            1 => {
                if let Some(p) = pending
                    && let Some(i) = conn.send_queue.iter().position(|&x| x == p)
                {
                    conn.send_queue[i] = m;
                }
            }
            2 => {
                conn.send_queue.push(m);
                if let Some(p) = pending {
                    remove(&mut conn.outstanding, &p);
                }
            }
            _ => {}
        }
        if let Some(p) = pending {
            self.message_destroy(p);
        }
        let ch = &mut self.conns[c].channels.as_mut().unwrap()[channel];
        ch.pending = Some(m);
        ch.where_ = 1;
        0
    }

    /// `connection_send_internal`.
    pub(crate) fn connection_send_internal(&mut self, c: ConnId, data: &[u8], ty: u8) -> i32 {
        if self.connection_is_terminated(c) {
            return 0;
        }
        let err = match ty {
            TYPE_BEST_EFFORT => self.send_best_effort(c, data),
            TYPE_GUARANTEED | TYPE_SYSTEM => self.send_guaranteed(c, data, ty),
            _ => self.send_state(c, data, ty as usize),
        };
        if err == 0 {
            self.route_resort(c);
            0
        } else {
            self.connection_terminate_reason(c, 1, 7);
            err
        }
    }

    /// `connection_send`: 251 guaranteed, 249 system, 252 best effort, and
    /// under 248 a state channel.
    pub fn connection_send(&mut self, c: ConnId, data: &[u8], ty: u8) {
        self.connection_send_internal(c, data, ty);
    }

    fn get_ack_mask(&self, c: ConnId) -> u64 {
        let conn = &self.conns[c];
        let mut mask = 0u64;
        for &m in &conn.delayed {
            let bit = u16_diff(self.msgs[m].seqnum, conn.rx_seqnum) - 2;
            if bit < 64 {
                mask |= 1u64.wrapping_shl(bit as u32);
            } else {
                break;
            }
        }
        mask
    }

    /// `connection_rx_reset`.
    pub(crate) fn connection_rx_reset(&mut self, c: ConnId) {
        let mut drop = Vec::new();
        let conn = &mut self.conns[c];
        conn.instance_remote = 0;
        conn.flags &= 0xffdf;
        conn.rx_flag = 0;
        conn.arrival_number = 0;
        conn.rx_seqnum = 0;
        if let Some(chs) = &mut conn.channels {
            for ch in chs.iter_mut() {
                ch.rx = 0;
                ch.rx_flag = 0;
                drop.append(&mut ch.rx_list);
            }
        }
        for m in drop {
            self.message_destroy(m);
        }
    }

    /// `connection_tx_reset`.
    fn connection_tx_reset(&mut self, c: ConnId) {
        let ep = self.conns[c].endpoint;
        let inst = self.endpoint_get_instance(ep);
        let mut drop = Vec::new();
        let conn = &mut self.conns[c];
        conn.instance_local = inst;
        conn.flags &= 0xfdbf;
        conn.time_instance_sent = 0;
        conn.time_last_flush = 0;
        conn.flush = 1;
        conn.tx_seqnum = 1;
        conn.flush_ack = 0;
        conn.rx_mask = 0;
        conn.tx_ack = 0;
        conn.tx_mask = 0;
        if let Some(chs) = &mut conn.channels {
            for ch in chs.iter_mut() {
                ch.tx = 1;
                ch.tx_ack = 0;
                drop.append(&mut ch.tx_list);
            }
        }
        for m in drop {
            self.message_destroy(m);
        }
    }

    /// `connection_append_message`: a best-effort or reliable message; the
    /// reliable ones (249-251) carry a sequence number and move to
    /// `outstanding`.
    fn append_message(&mut self, c: ConnId, b: &mut Buffer, m: MsgId, from_send_queue: bool, flush: u16) -> i32 {
        let (ty, size, seq) = (self.msgs[m].ty, self.msgs[m].data.len(), self.msgs[m].seqnum);
        let reliable = ty.wrapping_add(7) < 3;
        let cost = if ty.wrapping_add(7) <= 2 { size + 6 } else { size + 4 };
        if b.max < b.pos + cost {
            return -1;
        }
        self.list_remove_head(c, from_send_queue);
        b.put_u8(ty);
        b.put_size(size as u16);
        if reliable {
            b.put_u16(seq);
        }
        b.put_bytes(&self.msgs[m].data);
        if reliable {
            let t = self.time;
            self.msgs[m].time = t;
            self.msgs[m].flush = flush;
            self.conns[c].outstanding.push(m);
        } else {
            self.message_destroy(m);
        }
        0
    }

    fn list_remove_head(&mut self, c: ConnId, from_send_queue: bool) -> Option<MsgId> {
        let l = if from_send_queue { &mut self.conns[c].send_queue } else { &mut self.conns[c].outstanding };
        if l.is_empty() { None } else { Some(l.remove(0)) }
    }

    /// `connection_append_state`: the channel's message XOR'd against the
    /// acknowledged reference (if it is still held), RLE-coded.
    fn append_state(&mut self, c: ConnId, b: &mut Buffer, m: MsgId, from_send_queue: bool, flush: u16) -> i32 {
        let channel = self.msgs[m].ty as usize;
        let size = self.msgs[m].data.len() as u16;
        if b.pos > 300 {
            return -1;
        }
        if b.max < size as usize + b.pos + 27 {
            return -1;
        }
        let ch = &self.conns[c].channels.as_ref().unwrap()[channel];
        let next = ch.tx.wrapping_add(1);
        if next.wrapping_sub(ch.tx_ack) as i16 == 4096 {
            self.connection_terminate_reason(c, 1, 8);
            return -1;
        }
        if ch.where_ == 1 {
            let seq = ch.tx;
            self.conns[c].channels.as_mut().unwrap()[channel].tx = seq.wrapping_add(1);
            self.msgs[m].seqnum = seq;
            let msg = self.msgs[m].clone();
            let t = self.time;
            let copy = self.message_create(msg.endpoint, Some(c), msg.src, msg.dst, Some(&msg.data), msg.data.len(), msg.ty, msg.seqnum, t);
            self.conns[c].channels.as_mut().unwrap()[channel].tx_list.push(copy);
        }
        let seq = self.msgs[m].seqnum;
        let mut tx_list = std::mem::take(&mut self.conns[c].channels.as_mut().unwrap()[channel].tx_list);
        self.seqnum_remove_thru(&mut tx_list, seq.wrapping_sub(32));
        let tx_ack = self.conns[c].channels.as_ref().unwrap()[channel].tx_ack;
        let mut data = [0u8; 1300];
        let mut delta = 0u8;
        if let Some(&reference) = tx_list.first() {
            let msg = &self.msgs[m].data;
            if self.msgs[reference].seqnum == tx_ack {
                let refd = &self.msgs[reference].data;
                delta = seq.wrapping_sub(self.msgs[reference].seqnum) as u8;
                let common = msg.len().min(refd.len());
                for i in 0..common {
                    data[i] = msg[i] ^ refd[i];
                }
                data[common..msg.len()].copy_from_slice(&msg[common..]);
            } else {
                data[..msg.len()].copy_from_slice(msg);
            }
        }
        self.conns[c].channels.as_mut().unwrap()[channel].tx_list = tx_list;
        let sent = self.list_remove_head(c, from_send_queue).unwrap();
        b.put_u8(channel as u8);
        b.put_size(size);
        b.put_u16(seq);
        b.put_u8(delta);
        rle_encode(&data[..size as usize], b);
        self.conns[c].channels.as_mut().unwrap()[channel].where_ = 2;
        let t = self.time;
        self.msgs[sent].time = t;
        self.msgs[sent].flush = flush;
        self.conns[c].outstanding.push(sent);
        0
    }

    /// `connection_append`.
    fn append(&mut self, c: ConnId, b: &mut Buffer, from_send_queue: bool, flush: u16) -> i32 {
        let head = if from_send_queue { self.conns[c].send_queue[0] } else { self.conns[c].outstanding[0] };
        let ty = self.msgs[head].ty;
        if (TYPE_SYSTEM..TYPE_DISCONNECT).contains(&ty) {
            self.append_message(c, b, head, from_send_queue, flush)
        } else {
            self.append_state(c, b, head, from_send_queue, flush)
        }
    }

    /// `connection_message_get_desired_flush_time`: a resend is due a
    /// timeout after it went out, or at once if a later flush was acked.
    fn message_desired_flush_time(&self, c: ConnId, m: MsgId) -> u32 {
        let conn = &self.conns[c];
        let msg = &self.msgs[m];
        let mut t = msg.time.wrapping_add(conn.timeout as u32 + 100);
        if conn.flags & 0x10 != 0 && (conn.flush_ack.wrapping_sub(msg.flush) as i16) > 0 {
            t = msg.time;
        }
        t
    }

    /// `connection_get_next_message`: which list's head is due first. `None`
    /// for neither; `Some(true)` the send queue.
    fn get_next_message(&self, c: ConnId, time: &mut u32, mut have: bool) -> Option<bool> {
        let conn = &self.conns[c];
        let mut which = None;
        if let Some(&m) = conn.send_queue.first() {
            if have {
                if (self.msgs[m].time.wrapping_sub(*time) as i32) < 0 {
                    *time = self.msgs[m].time;
                    which = Some(true);
                }
            } else {
                have = true;
                *time = self.msgs[m].time;
                which = Some(true);
            }
        }
        if let Some(&m) = conn.outstanding.first() {
            let t = self.message_desired_flush_time(c, m);
            if have {
                if (t.wrapping_sub(*time) as i32) < 0 {
                    *time = t;
                    which = Some(false);
                }
            } else {
                *time = t;
                which = Some(false);
            }
        }
        which
    }

    /// `connection_idle_check` (software-to-software connections only).
    fn idle_check(&mut self, c: ConnId) -> i32 {
        if !self.connection_is_sw_sw(c) {
            return 0;
        }
        let t = self.time;
        let mut r = 0;
        let conn = &self.conns[c];
        if conn.flags & 0x200 != 0
            && conn.flags & 0x40 != 0
            && conn.send_queue.is_empty()
            && conn.outstanding.is_empty()
            && conn.flags & 0xf == 0
            && u32_ge(t, conn.time_last_flush.wrapping_add(IDLE_RESET))
        {
            self.connection_tx_reset(c);
            r = 1;
        }
        let conn = &self.conns[c];
        if conn.flags & 0x200 == 0 && conn.flags & 0x20 != 0 && conn.delayed.is_empty() && conn.arrivals.is_empty() && u32_ge(t, conn.time_last_arrival.wrapping_add(360000)) {
            r |= 2;
            self.endpoint_connection_destroy(c);
        }
        r
    }

    /// `connection_flush`: the segment body (flags, acks, state acks) and as
    /// many due messages as fit. 0 written, -1 nothing, -2 overflow, -3 the
    /// connection is gone.
    pub(crate) fn connection_flush(&mut self, c: ConnId, b: &mut Buffer, flush_number: i32) -> i32 {
        let flush = self.conns[c].flush;
        self.conns[c].last_route_flush_number = flush_number;
        let ep = self.conns[c].endpoint;
        if self.endpoint_is_closed(ep) && self.endpoint_is_in_sync(ep) {
            self.drdp_endpoint_destroy(ep);
            return -3;
        }
        if self.connection_is_closed(c) && self.connection_is_in_sync(c) {
            self.connection_terminate_reason(c, 1, 10);
            self.endpoint_connection_destroy(c);
            return -3;
        }
        let idle = self.idle_check(c);
        if idle & 2 != 0 {
            return -3;
        }
        if idle & 1 != 0 {
            return -1;
        }
        let t = self.time;
        if !self.connection_is_sw_sw(c) && !self.connection_is_closed(c) && !self.endpoint_is_closed(ep) && u32_ge(t, self.conns[c].time_send.wrapping_add(30000)) {
            self.connection_send_internal(c, &[18], TYPE_SYSTEM);
            if self.conns[c].flags & 0x100 != 0 {
                return -1;
            }
        }
        let mut ack_mask = 0u64;
        let mut f = 0u8;
        let fl = self.conns[c].flags;
        if fl & 1 != 0 || fl & 2 != 0 {
            ack_mask = self.get_ack_mask(c);
            f |= 2;
            if ack_mask != 0 {
                f |= 8;
            }
        }
        if fl & 4 != 0 {
            f |= 0x10;
        }
        if fl & 1 != 0 {
            f |= 1;
            if self.conns[c].rx_mask != 0 {
                f |= 4;
            }
        }
        if fl & 8 != 0 {
            f |= 0x40;
        }
        if self.conns[c].rx_flag < 17 {
            f |= 0x20;
        }
        let conn = &mut self.conns[c];
        b.put_u8(f);
        if f & 0x40 != 0 {
            b.put_u32(conn.instance_remote);
        }
        b.put_u16(flush);
        if f & 1 != 0 {
            b.put_u16(conn.arrival_number);
            if f & 4 != 0 {
                b.put_packed(conn.rx_mask);
            }
        }
        if f & 2 != 0 {
            b.put_u16(conn.rx_seqnum);
            if f & 8 != 0 {
                b.put_packed(ack_mask);
            }
        }
        if f & 0x10 != 0 {
            let chs = conn.channels.as_mut().unwrap();
            for (i, ch) in chs.iter_mut().enumerate() {
                if ch.rx_flag != 0 {
                    b.put_u8(i as u8);
                    b.put_u16(ch.rx);
                    ch.rx_flag -= 1;
                }
            }
            b.put_u8(CHANNELS as u8);
        }
        if b.overflow {
            return -2;
        }
        loop {
            let mut when = 0u32;
            let Some(from_send_queue) = self.get_next_message(c, &mut when, false) else { break };
            let conn = &self.conns[c];
            if is_hardware_endpoint(conn.local_id) || is_hardware_endpoint(conn.remote_id) {
                let head = if from_send_queue { conn.send_queue[0] } else { conn.outstanding[0] };
                let age = self.time.wrapping_sub(self.msgs[head].created);
                let silence = self.time.wrapping_sub(conn.time_last_arrival);
                let timeout = if conn.flags & 0x40 != 0 {
                    if conn.route.is_some_and(|r| self.routes[r].terminated) { conn.retry_time } else { conn.warn_time }
                } else {
                    conn.retry_time
                };
                if (conn.fail_time as u32) < age {
                    self.connection_terminate_reason(c, 1, 3);
                    return -1;
                }
                if !from_send_queue && timeout < age as i32 && timeout < silence as i32 {
                    self.connection_terminate_reason(c, 1, 2);
                    return -1;
                }
            }
            if self.time < when {
                break;
            }
            if self.append(c, b, from_send_queue, flush) != 0 {
                break;
            }
        }
        let t = self.time;
        let conn = &mut self.conns[c];
        if conn.flags & 0x40 == 0 {
            conn.timeout = conn.timeout.wrapping_add(1000);
        }
        if conn.flags & 0x200 == 0 {
            conn.time_instance_sent = t;
            conn.time_last_arrival = t;
            conn.flags |= 0x200;
        }
        conn.flags &= 0xfff0;
        conn.flush = conn.flush.wrapping_add(1);
        conn.cstat[6] += 1;
        conn.flush_time[(flush & 1023) as usize] = t;
        conn.time_last_flush = t;
        0
    }

    fn add_rtt_sample(&mut self, c: ConnId, sample: u16) {
        let conn = &mut self.conns[c];
        conn.rtt[conn.rtt_index as usize] = sample;
        conn.timeout = conn.rtt[0];
        for i in 1..16 {
            conn.timeout = if conn.timeout < conn.rtt[i] { conn.rtt[i] } else { conn.timeout };
        }
        conn.rtt_index = (conn.rtt_index + 1) % 16;
    }

    /// `connection_recalc_desired_flush_time`.
    pub(crate) fn connection_recalc_desired_flush_time(&mut self, c: ConnId) {
        let t = self.time;
        let sw_sw = self.connection_is_sw_sw(c);
        let closed_or_terminated = self.connection_is_closed(c) || self.connection_is_terminated(c);
        let conn = &mut self.conns[c];
        conn.desired_flush_time = if sw_sw {
            if conn.flags & 0x200 != 0 {
                conn.time_last_flush.wrapping_add(120000)
            } else if conn.flags & 0x20 != 0 {
                conn.time_last_arrival.wrapping_add(360000)
            } else {
                conn.time_last_arrival.wrapping_add(ONE_DAY)
            }
        } else if closed_or_terminated {
            t.wrapping_add(ONE_DAY)
        } else {
            conn.time_send.wrapping_add(30000)
        };
        let mut when = self.conns[c].desired_flush_time;
        self.get_next_message(c, &mut when, true);
        let conn = &mut self.conns[c];
        conn.desired_flush_time = when;
        if conn.flags & 0xf != 0 && (conn.desired_flush_time.wrapping_sub(conn.flush_by) as i32) >= 0 {
            conn.desired_flush_time = conn.flush_by;
        }
        if self.connection_is_closed(c) && self.connection_is_in_sync(c) {
            self.conns[c].desired_flush_time = self.conns[c].time_last_arrival;
        }
        let ep = self.conns[c].endpoint;
        if self.endpoint_is_closed(ep) && self.endpoint_is_in_sync(ep) {
            self.conns[c].desired_flush_time = self.conns[c].time_last_arrival;
        }
    }

    fn record_guaranteed_ack(&mut self, c: ConnId, seqnum: u16, mask: u64) {
        for m in self.conns[c].outstanding.clone() {
            let ty = self.msgs[m].ty;
            if ty == TYPE_GUARANTEED || ty == TYPE_FRAGMENT || ty == TYPE_SYSTEM {
                let ahead = u16_diff(self.msgs[m].seqnum, seqnum);
                let acked = if ahead <= 0 {
                    true
                } else if ahead != 1 {
                    let bit = (ahead - 2) as u32;
                    bit < 64 && (1u64 << bit) & mask != 0
                } else {
                    false
                };
                if acked {
                    remove(&mut self.conns[c].outstanding, &m);
                    self.message_destroy(m);
                }
            }
        }
        self.conns[c].tx_ack = seqnum;
        self.conns[c].tx_mask = mask;
    }

    fn record_state_acks(&mut self, c: ConnId, acks: &[(u8, u16)]) {
        for &(channel, seqnum) in acks {
            if channel as usize == CHANNELS {
                break;
            }
            let ch = &self.conns[c].channels.as_ref().unwrap()[channel as usize];
            if (seqnum.wrapping_sub(ch.tx_ack) as i16) > 0 {
                if ch.where_ == 2
                    && let Some(p) = ch.pending
                    && self.msgs[p].seqnum == seqnum
                {
                    let conn = &mut self.conns[c];
                    conn.channels.as_mut().unwrap()[channel as usize].where_ = 0;
                    remove(&mut conn.outstanding, &p);
                    conn.channels.as_mut().unwrap()[channel as usize].pending = None;
                    self.message_destroy(p);
                }
                let ch = &mut self.conns[c].channels.as_mut().unwrap()[channel as usize];
                ch.tx_ack = seqnum;
                let mut tx = std::mem::take(&mut ch.tx_list);
                self.seqnum_remove_thru(&mut tx, seqnum);
                self.conns[c].channels.as_mut().unwrap()[channel as usize].tx_list = tx;
            }
        }
    }

    fn record_flush_ack(&mut self, c: ConnId, flush: u16) {
        let conn = &mut self.conns[c];
        if u16_lt(flush, conn.flush.wrapping_sub(1024)) {
            return;
        }
        let slot = &mut conn.flush_time[(flush & 1023) as usize];
        if *slot != 0 {
            let rtt = self.time.wrapping_sub(*slot);
            *slot = 0;
            self.add_rtt_sample(c, if rtt > 0xffff { 0xffff } else { rtt as u16 });
        }
    }

    fn record_acks(&mut self, c: ConnId, body: &Body) {
        if body.flags & 0x40 != 0 {
            self.conns[c].flags |= 0x40;
        }
        if body.flags & 1 != 0 {
            self.record_flush_ack(c, body.flush);
            if body.flags & 4 != 0 {
                let mut mask = body.flush_mask;
                let mut flush = body.flush;
                while mask != 0 {
                    flush = flush.wrapping_sub(1);
                    if mask & 1 != 0 {
                        self.record_flush_ack(c, flush);
                    }
                    mask >>= 1;
                }
            }
            if (body.flush.wrapping_sub(self.conns[c].flush_ack) as i16) > 0 {
                self.conns[c].flush_ack = body.flush;
            }
        }
        if body.guaranteed_ack {
            self.record_guaranteed_ack(c, body.seqnum, body.mask);
        }
        if body.flags & 0x10 != 0 {
            self.record_state_acks(c, &body.state_acks);
        }
    }

    /// `connection_move_delayed_to_arrivals`: in-order reliable messages are
    /// delivered, fragments collected until the last one completes them.
    fn move_delayed_to_arrivals(&mut self, c: ConnId) {
        while let Some(&m) = self.conns[c].delayed.first() {
            if self.msgs[m].seqnum != self.conns[c].rx_seqnum.wrapping_add(1) {
                break;
            }
            let ep = self.conns[c].endpoint;
            self.conns[c].rx_seqnum = self.conns[c].rx_seqnum.wrapping_add(1);
            self.conns[c].delayed.remove(0);
            if self.msgs[m].ty == TYPE_FRAGMENT {
                self.conns[c].arrivals.push(m);
            } else if self.conns[c].arrivals.is_empty() {
                self.endpoint_arrival_enqueue(ep, m);
            } else {
                self.conns[c].arrivals.push(m);
                let pieces = std::mem::take(&mut self.conns[c].arrivals);
                let total: usize = pieces.iter().map(|&p| self.msgs[p].data.len()).sum();
                debug_assert!(total <= SEND_SIZE_MAX_GUARANTEED);
                let mut data = Vec::with_capacity(total);
                for &p in &pieces {
                    data.extend_from_slice(&self.msgs[p].data);
                }
                let (l, r, ty, first, t) = (self.conns[c].local_id, self.conns[c].remote_id, self.msgs[m].ty, self.msgs[pieces[0]].seqnum, self.time);
                let whole = self.message_create(Some(ep), Some(c), r, l, Some(&data), total, ty, first, t);
                for p in pieces {
                    self.message_destroy(p);
                }
                self.endpoint_arrival_enqueue(ep, whole);
            }
        }
    }

    /// `connection_record_state_message`.
    fn record_state_message(&mut self, c: ConnId, m: MsgId, with: Option<MsgId>) {
        let seq = self.msgs[m].seqnum;
        let channel = self.msgs[m].ty as usize;
        let refnum = self.msgs[m].refnum_offset;
        let ch = &mut self.conns[c].channels.as_mut().unwrap()[channel];
        ch.rx = seq;
        ch.rx_flag = 2;
        let mut rx = std::mem::take(&mut ch.rx_list);
        self.seqnum_remove_thru(&mut rx, if refnum == 0 { seq.wrapping_sub(32) } else { seq.wrapping_sub(refnum as u16) });
        if let Some(w) = with {
            rx.push(w);
        }
        self.conns[c].channels.as_mut().unwrap()[channel].rx_list = rx;
        let ep = self.conns[c].endpoint;
        self.endpoint_arrival_enqueue(ep, m);
    }

    /// `connection_record_message`.
    fn record_message(&mut self, c: ConnId, m: MsgId, rest: &mut Vec<MsgId>) {
        match self.msgs[m].ty {
            TYPE_BEST_EFFORT | 254 | 255 => {
                let ep = self.conns[c].endpoint;
                self.endpoint_arrival_enqueue(ep, m);
            }
            TYPE_SYSTEM | TYPE_FRAGMENT | TYPE_GUARANTEED => {
                let mut d = std::mem::take(&mut self.conns[c].delayed);
                self.insert_by_seqnum(&mut d, m);
                self.conns[c].delayed = d;
                self.move_delayed_to_arrivals(c);
            }
            _ => {
                let with = if rest.is_empty() { None } else { Some(rest.remove(0)) };
                self.record_state_message(c, m, with);
            }
        }
    }

    /// `connection_record_segment`.
    fn record_segment(&mut self, c: ConnId, h: &SegmentHeader, body: &mut Body) {
        let t = self.time;
        let conn = &mut self.conns[c];
        conn.time_last_arrival = t;
        if h.flags & 0x2000 != 0 {
            conn.time_last_instance_arrival = t;
            if conn.flags & 0x20 == 0 {
                conn.flags |= 0x20;
                conn.instance_remote = h.crc;
            }
            if h.flags & 0x80000 != 0 {
                if conn.flags & 0xf == 0 {
                    conn.flush_by = t.wrapping_add(100);
                }
                conn.flags |= 9;
            }
        }
        if body.arrival {
            conn.rx_flag = conn.rx_flag.saturating_add(1);
            if body.flags & 0x20 != 0 {
                conn.flags &= 0xffef;
            } else {
                conn.flags |= 0x10;
            }
        } else {
            conn.rx_flag = 0;
        }
        if body.arrival {
            conn.arrival_history = (conn.arrival_history << 1) | 1;
            conn.arrival_number = conn.arrival_number.wrapping_add(1);
            if u16_gt(body.arrival_number, conn.arrival_number) {
                conn.arrival_history = conn.arrival_history.wrapping_shl(u16_diff(body.arrival_number, conn.arrival_number) as u32);
            }
            conn.arrival_number = body.arrival_number;
        } else {
            let shift = u16_diff(conn.arrival_number, body.arrival_number) - 1;
            conn.arrival_history |= 1u64.wrapping_shl(shift as u32);
        }
        if body.acks {
            self.record_acks(c, body);
        }
        // connection_record_segment_messages
        if body.conn_flags != 0 {
            let conn = &mut self.conns[c];
            if conn.flags & 0xf == 0 {
                conn.flush_by = t.wrapping_add(100);
            }
            conn.flags |= body.conn_flags;
            for &ch in &body.channels {
                conn.channels.as_mut().unwrap()[ch as usize].rx_flag = 2;
            }
        }
        let mut list = std::mem::take(&mut body.messages);
        while !list.is_empty() {
            let m = list.remove(0);
            self.record_message(c, m, &mut list);
        }
        self.route_resort(c);
    }

    fn has_reliable(&self, c: ConnId, seq: u16) -> bool {
        let conn = &self.conns[c];
        let by = |x: MsgId| self.msg_seq(x);
        let cmp = |a: u64, b: u64| cmp_u16_wrap(a as u16, b as u16);
        u16_le(seq, conn.rx_seqnum) || lookup_sorted(&conn.arrivals, seq as u64, by, cmp).is_some() || lookup_sorted(&conn.delayed, seq as u64, by, cmp).is_some()
    }

    /// `connection_parse_segment_body`. The header checks fail with -1; the
    /// message loop's failures jump out before setting a result and leave the
    /// original's initial `0x80000000`.
    fn parse_segment_body(&mut self, c: ConnId, h: &SegmentHeader, body: &mut Body, b: &mut Buffer) -> Result<(), i32> {
        self.conns[c].cstat[5] += 1;
        if h.flags & 0x2000 != 0 {
            let conn = &mut self.conns[c];
            if conn.flags & 0x20 != 0 {
                if conn.instance_remote != h.crc {
                    conn.cstat[0] += 1;
                    return Err(-1);
                }
            } else if h.flags & 0x80000 == 0 {
                conn.cstat[2] += 1;
                return Err(-1);
            }
        }
        body.flags = b.u8();
        if body.flags & 0x40 != 0 {
            body.instance = b.u32();
        }
        body.acks = true;
        if self.conns[c].flags & 0x40 == 0 && (body.flags & 0x40 == 0 || body.instance != self.conns[c].instance_local) {
            body.acks = false;
        }
        body.arrival = true;
        body.arrival_number = b.u16();
        {
            let conn = &mut self.conns[c];
            let inc = u16_diff(body.arrival_number, conn.arrival_number);
            if inc <= 0 {
                let back = !inc;
                let bit = 1u64.wrapping_shl(back as u32);
                if inc == 0 || bit & conn.arrival_history != 0 {
                    return Err(-1);
                }
                if back >= 64 {
                    conn.cstat[0] += 1;
                    return Err(-1);
                }
                body.arrival = false;
            } else if inc > 4096 {
                conn.cstat[2] += 1;
                return Err(-1);
            }
        }
        if body.flags & 1 != 0 {
            body.flush = b.u16();
            if body.flags & 4 != 0 {
                body.flush_mask = b.packed();
            }
            if body.acks && u16_ge(body.flush, self.conns[c].flush) {
                self.conns[c].cstat[2] += 1;
                return Err(-1);
            }
        }
        body.guaranteed_ack = false;
        if body.flags & 2 != 0 {
            body.guaranteed_ack = body.acks;
            body.seqnum = b.u16();
            if body.flags & 8 != 0 {
                body.mask = b.packed();
            }
            if body.acks {
                let conn = &mut self.conns[c];
                if u16_ge(body.seqnum, conn.tx_seqnum) {
                    conn.cstat[2] += 1;
                    return Err(-1);
                }
                if u16_lt(body.seqnum, conn.tx_ack) {
                    body.guaranteed_ack = false;
                }
                let acked = u16_diff(body.seqnum, conn.tx_ack);
                if acked > 4096 {
                    conn.cstat[2] += 1;
                    return Err(-1);
                }
                let mask = if acked >= 64 { 0 } else { conn.tx_mask.wrapping_shr(acked as u32) };
                if (body.mask ^ mask) & mask != 0 {
                    body.guaranteed_ack = false;
                }
                if body.arrival && !body.guaranteed_ack {
                    conn.cstat[2] += 1;
                    return Err(-1);
                }
            }
        }
        if body.flags & 0x10 != 0 {
            let mut i = 0usize;
            loop {
                let ch = b.u8();
                body.state_acks.push((ch, 0));
                if ch as usize == CHANNELS {
                    break;
                }
                if ch as usize > CHANNELS || (i > 0 && body.state_acks[i - 1].0 >= ch) || i == CHANNELS {
                    self.conns[c].cstat[2] += 1;
                    return Err(-1);
                }
                body.state_acks[i].1 = b.u16();
                i += 1;
            }
            if body.acks {
                let conn = &mut self.conns[c];
                let Some(chs) = &conn.channels else {
                    conn.cstat[2] += 1;
                    return Err(-1);
                };
                for &(ch, ack) in &body.state_acks {
                    if ch as usize == CHANNELS {
                        break;
                    }
                    let s = &chs[ch as usize];
                    if u16_ge(ack, s.tx) || (body.arrival && u16_lt(ack, s.tx_ack)) {
                        conn.cstat[2] += 1;
                        return Err(-1);
                    }
                }
            }
        }
        if (is_hardware_endpoint(h.src) || is_hardware_endpoint(h.dst)) && self.conns[c].flags & 0x20 == 0 {
            let (ep, t) = (self.conns[c].endpoint, self.time);
            let reset = self.message_create(Some(ep), Some(c), h.src, h.dst, None, 0, 255, 0xffff, t);
            body.messages.push(reset);
        }
        while b.remaining() != 0 {
            let ty = b.u8();
            let size = b.size();
            if size >= 2047 {
                return Err(i32::MIN);
            }
            let (ep, t) = (self.conns[c].endpoint, self.time);
            if ty.wrapping_add(7) < 3 {
                let seq = b.u16();
                body.conn_flags |= 3;
                if u16_gt(seq, self.conns[c].rx_seqnum.wrapping_add(4096)) {
                    self.conns[c].cstat[2] += 1;
                    return Err(i32::MIN);
                }
                if self.has_reliable(c, seq) {
                    b.skip(size as usize);
                    self.conns[c].cstat[7] += 1;
                    continue;
                }
                if body.messages.iter().any(|&m| self.msgs[m].ty == ty && self.msgs[m].seqnum == seq) {
                    self.conns[c].cstat[2] += 1;
                    return Err(i32::MIN);
                }
                let m = self.message_create(Some(ep), Some(c), h.src, h.dst, None, size as usize, ty, seq, t);
                self.conns[c].cstat[8] += 1;
                let mut d = vec![0u8; size as usize];
                b.read_into(&mut d);
                self.msgs[m].data = d;
                body.messages.push(m);
            } else if ty == TYPE_BEST_EFFORT {
                if body.arrival {
                    let m = self.message_create(Some(ep), Some(c), h.src, h.dst, None, size as usize, ty, 0xffff, t);
                    let mut d = vec![0u8; size as usize];
                    b.read_into(&mut d);
                    self.msgs[m].data = d;
                    body.messages.push(m);
                } else {
                    b.skip(size as usize);
                }
            } else if ty < TYPE_STATE_CHANNELS {
                body.conn_flags |= 5;
                self.allocate_state_channels(c);
                let seq = b.u16();
                let refnum = b.u8();
                if refnum > 32 {
                    self.conns[c].cstat[2] += 1;
                    return Err(i32::MIN);
                }
                let rx = self.conns[c].channels.as_ref().unwrap()[ty as usize].rx;
                if u16_le(seq, rx) {
                    self.conns[c].cstat[9] += 1;
                    let mut scratch = vec![0u8; size as usize];
                    rle_decode(b, &mut scratch);
                    body.channels.push(ty);
                    continue;
                }
                if (seq.wrapping_sub(rx) as i16) as i32 > 4096 {
                    self.conns[c].cstat[2] += 1;
                    return Err(i32::MIN);
                }
                let m = self.message_create(Some(ep), Some(c), h.src, h.dst, None, size as usize, ty, seq, t);
                self.msgs[m].refnum_offset = refnum;
                body.messages.push(m);
                let mut d = vec![0u8; size as usize];
                rle_decode(b, &mut d);
                if refnum != 0 {
                    let refseq = seq.wrapping_sub(refnum as u16);
                    let list = &self.conns[c].channels.as_ref().unwrap()[ty as usize].rx_list;
                    let Some(r) = lookup_sorted(list, refseq as u64, |x| self.msg_seq(x), |a, b| cmp_u16_wrap(a as u16, b as u16)) else {
                        self.conns[c].cstat[2] += 1;
                        return Err(i32::MIN);
                    };
                    let refd = &self.msgs[r].data;
                    let n = d.len().min(refd.len());
                    for i in 0..n {
                        d[i] ^= refd[i];
                    }
                }
                self.msgs[m].data = d;
                let copy = self.msgs[m].clone();
                let dup = self.msgs.insert(copy);
                self.conns[c].cstat[10] += 1;
                body.messages.push(dup);
            } else {
                return Err(i32::MIN);
            }
        }
        if !body.arrival {
            self.conns[c].cstat[4] += 1;
        }
        Ok(())
    }

    /// `connection_handle_segment`.
    pub(crate) fn connection_handle_segment(&mut self, c: ConnId, h: &SegmentHeader, b: &mut Buffer) -> i32 {
        let mut body = Body::default();
        let r = self.parse_segment_body(c, h, &mut body, b);
        let result = match r {
            Ok(()) => {
                if body.arrival && self.is_client() && is_software_endpoint(h.src) {
                    self.re_route_connections_to_endpoint(h.src, h.route);
                }
                self.record_segment(c, h, &mut body);
                0
            }
            Err(e) => e,
        };
        for m in body.messages {
            self.message_destroy(m);
        }
        result
    }

    /// `connection_send_instance_local`: until the far end acks our instance,
    /// and for two minutes after it was sent.
    pub(crate) fn connection_send_instance_local(&self, c: ConnId) -> bool {
        let conn = &self.conns[c];
        let when = conn.time_instance_sent.wrapping_add(120000);
        self.connection_is_sw_sw(c) || conn.flags & 0x40 == 0 || u32_gt(when, self.time)
    }

    /// `connection_send_reset`.
    pub(crate) fn connection_send_reset(&mut self, c: ConnId) {
        let conn = &self.conns[c];
        let reason = if conn.flags & 0x20 != 0 { 3 } else { 1 };
        let (r, l, d, il, ir) = (conn.route, conn.local_id, conn.remote_id, conn.instance_local, conn.instance_remote);
        if let Some(r) = r {
            self.route_send_reset_connection(r, l, d, reason, il, ir);
        }
    }

    /// `connection_terminate`: the pre-made DISCONNECT message delivered with
    /// `data` in it, everything outstanding dropped, and a reset sent if asked.
    pub(crate) fn connection_terminate(&mut self, c: ConnId, send_reset: i32, data: &[u8]) {
        if self.conns[c].flags & 0x100 != 0 {
            return;
        }
        let t = self.time;
        if let Some(m) = self.conns[c].terminate.take() {
            self.msgs[m].time = t;
            let n = data.len().min(self.msgs[m].data.len());
            self.msgs[m].data[..n].copy_from_slice(&data[..n]);
            let ep = self.conns[c].endpoint;
            self.endpoint_arrival_enqueue(ep, m);
        }
        self.connection_free_outstanding(c);
        self.conns[c].flags |= 0x100;
        if send_reset & 1 != 0 {
            self.connection_send_reset(c);
        }
    }

    pub(crate) fn connection_terminate_reason(&mut self, c: ConnId, send_reset: i32, reason: u8) {
        self.connection_terminate(c, send_reset, &[reason]);
    }

    /// `connection_is_in_sync`: no reliable message left to deliver (sticky).
    pub(crate) fn connection_is_in_sync(&mut self, c: ConnId) -> bool {
        let conn = &self.conns[c];
        if conn.flags & 0x100 != 0 || conn.flags & 0x1000 != 0 {
            return true;
        }
        let reliable = |&m: &MsgId| matches!(self.msgs[m].ty, TYPE_GUARANTEED | TYPE_FRAGMENT | TYPE_SYSTEM);
        if !conn.send_queue.iter().any(reliable) && !conn.outstanding.iter().any(reliable) {
            self.conns[c].flags |= 0x1000;
            return true;
        }
        false
    }

    /// `connection_handle_reset`.
    pub(crate) fn connection_handle_reset(&mut self, c: ConnId, h: &SegmentHeader) -> i32 {
        let conn = &self.conns[c];
        if h.flags & 0x2000 != 0 && conn.flags & 0x20 != 0 && conn.instance_remote != h.crc {
            return -1;
        }
        if h.flags & 0x40000 != 0 && conn.instance_local != h.flush_number {
            return -1;
        }
        if h.flags & 0x2000 == 0 && h.flags & 0x40000 == 0 && self.connection_send_instance_local(c) {
            return -1;
        }
        self.connection_terminate_reason(c, 0, 6);
        0
    }

    /// `connection_open`: to `dst`, from `ep`, with a fresh instance.
    pub fn connection_open(&mut self, ep: EpId, dst: u64) -> Option<ConnId> {
        let route = self.get_route_endpoint(dst);
        let inst = self.endpoint_get_instance(ep);
        self.endpoint_connection_create(ep, dst, route, inst, 1).ok()
    }

    /// `connection_close`: its arrivals dropped, then gone once in sync.
    pub fn connection_close(&mut self, c: ConnId) {
        self.endpoint_connection_free_arrivals(c);
        self.conns[c].flags |= 0x800;
        if self.connection_is_in_sync(c) {
            self.connection_terminate_reason(c, 1, 10);
            self.endpoint_connection_destroy(c);
        }
    }

    pub fn connection_set_failure(&mut self, c: ConnId, retry: i32, warn: i32, fail: i32) {
        let conn = &mut self.conns[c];
        conn.retry_time = retry;
        conn.warn_time = warn;
        conn.fail_time = fail;
    }

    pub fn connection_set_simulated(&mut self, c: ConnId, drop: i32, delay: i32, jitter: i32) {
        if let Some(r) = self.conns[c].route {
            let route = &mut self.routes[r];
            route.drop = drop;
            route.delay = delay;
            route.jitter = jitter;
        }
    }

    /// `connection_set_bps_limit` (`route_set_bps_limit`).
    pub fn connection_set_bps_limit(&mut self, c: ConnId, limit: i32) {
        let Some(r) = self.conns[c].route else { return };
        let t = self.time;
        let interval = self.flush_interval();
        let route = &mut self.routes[r];
        route.bandwidth.set_bps_limit(t, limit);
        route.bandwidth.queue_size_max = vi::fpu::F::new(((vi::fpu::F::<M>::new(limit as f32) * interval).0 as i32) as f32);
    }

    /// `connection_get_ping_time`: the smallest of the sixteen samples.
    pub fn connection_ping(&self, c: ConnId) -> i32 {
        let conn = &self.conns[c];
        conn.rtt.iter().fold(conn.rtt[0] as i32, |p, &r| p.min(r as i32))
    }
}
