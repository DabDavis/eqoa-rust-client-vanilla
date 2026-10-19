//! DRDP against the decomp, byte for byte: eqoa-port's `build/oracle_drdp`
//! runs the decomp's network library with only the socket replaced, and
//! every step here is run on it and on this crate, and the replies compared.
//!
//! Two scenarios: two instances talking to each other through a lossy,
//! reordering, duplicating network for thousands of steps (guaranteed
//! messages big enough to fragment, state channels with small changes,
//! best-effort, clock jumps that trip the keep-alive and the timeouts); and
//! a real capture of a PS2 talking to Sandstorm replayed into one client.
//!
//! SKIPs without the oracle (`make oracle` in eqoa-port).

use drdp::{Addr, Drdp, address_to_endpoint, PORT_LEADER};
use std::io::{Read, Write};
use std::process::{Child, ChildStdin, ChildStdout, Command, Stdio};
use vi::fpu::Ieee;

struct Oracle {
    _child: Child,
    stdin: ChildStdin,
    stdout: ChildStdout,
}

impl Oracle {
    fn open() -> Option<Oracle> {
        let path = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../../eqoa-port/build/oracle_drdp");
        if !path.exists() {
            eprintln!("SKIP: no oracle at {} (make oracle in eqoa-port)", path.display());
            return None;
        }
        let mut child = Command::new(path).stdin(Stdio::piped()).stdout(Stdio::piped()).spawn().ok()?;
        let stdin = child.stdin.take()?;
        let stdout = child.stdout.take()?;
        Some(Oracle { _child: child, stdin, stdout })
    }

    fn call(&mut self, op: u32, payload: &[u8]) -> Vec<u8> {
        self.stdin.write_all(&op.to_le_bytes()).unwrap();
        self.stdin.write_all(&(payload.len() as u32).to_le_bytes()).unwrap();
        self.stdin.write_all(payload).unwrap();
        self.stdin.flush().unwrap();
        let mut n = [0u8; 4];
        self.stdout.read_exact(&mut n).expect("the oracle died");
        let mut out = vec![0u8; u32::from_le_bytes(n) as usize];
        self.stdout.read_exact(&mut out).unwrap();
        out
    }
}

struct Rng(u64);
impl Rng {
    fn next(&mut self) -> u32 {
        self.0 = self.0.wrapping_mul(6364136223846793005).wrapping_add(1442695040888963407);
        (self.0 >> 33) as u32
    }
    fn below(&mut self, n: u32) -> u32 {
        self.next() % n.max(1)
    }
    fn chance(&mut self, percent: u32) -> bool {
        self.below(100) < percent
    }
}

/// One step, as bytes for the oracle and run on the Rust side, which
/// answers in the same format.
enum Op<'a> {
    Create(Addr, u32),
    Time(u32),
    Open(u64, i32, i32, i32),
    Send(u64, u8, &'a [u8]),
    Flush,
    Packet(&'a [u8]),
    Receive,
    State(u64),
    Close(u64),
    Next,
    #[allow(dead_code)]
    SendAddr(Addr, &'a [u8]),
}

fn packets(out: &mut Vec<u8>, ps: &[drdp::Packet]) {
    out.extend((ps.len() as i32).to_le_bytes());
    for p in ps {
        out.extend(p.to.ip);
        out.extend(p.to.port.to_le_bytes());
        out.extend((p.data.len() as i32).to_le_bytes());
        out.extend(&p.data);
    }
}

fn run(o: &mut Oracle, rs: &mut [Option<Drdp<Ieee>>; 4], i: usize, op: &Op) -> (Vec<u8>, Vec<u8>) {
    let mut req = (i as u32).to_le_bytes().to_vec();
    let mut mine = Vec::new();
    let code = match op {
        Op::Create(a, t) => {
            req.extend(a.ip);
            req.extend((a.port as u32).to_le_bytes());
            req.extend(t.to_le_bytes());
            let d = Drdp::<Ieee>::create_client(*a, *t);
            mine.extend(d.port_drdp().to_le_bytes());
            rs[i] = Some(d);
            700
        }
        Op::Time(t) => {
            req.extend(t.to_le_bytes());
            rs[i].as_mut().unwrap().set_time(*t);
            701
        }
        Op::Open(dst, r, w, f) => {
            req.extend(dst.to_le_bytes());
            req.extend(r.to_le_bytes());
            req.extend(w.to_le_bytes());
            req.extend(f.to_le_bytes());
            let d = rs[i].as_mut().unwrap();
            let sys = d.system_endpoint;
            let res: i32 = match d.connection_open(sys, *dst) {
                Some(c) => {
                    d.connection_set_failure(c, *r, *w, *f);
                    d.connection_set_simulated(c, 0, 0, 0);
                    0
                }
                None => -1,
            };
            mine.extend(res.to_le_bytes());
            702
        }
        Op::Send(dst, ty, data) => {
            req.extend(dst.to_le_bytes());
            req.extend((*ty as u32).to_le_bytes());
            req.extend((data.len() as u32).to_le_bytes());
            req.extend(*data);
            let d = rs[i].as_mut().unwrap();
            let c = d.endpoint_find_connection(d.system_endpoint, *dst);
            if let Some(c) = c {
                d.connection_send(c, data, *ty);
            }
            mine.extend((c.is_some() as i32).to_le_bytes());
            703
        }
        Op::Flush => {
            let d = rs[i].as_mut().unwrap();
            d.flush();
            let sent = std::mem::take(&mut d.outbox);
            packets(&mut mine, &sent);
            704
        }
        Op::Packet(data) => {
            req.extend((data.len() as u32).to_le_bytes());
            req.extend(*data);
            let d = rs[i].as_mut().unwrap();
            let r = d.process_packet(data);
            mine.extend(r.to_le_bytes());
            let sent = std::mem::take(&mut d.outbox);
            packets(&mut mine, &sent);
            705
        }
        Op::Receive => {
            let d = rs[i].as_mut().unwrap();
            let mut n = 0i32;
            let mut body = Vec::new();
            while let Some(m) = d.receive() {
                body.push(m.ty);
                body.extend(m.src.to_le_bytes());
                body.extend(m.dst.to_le_bytes());
                body.extend(m.seqnum.to_le_bytes());
                body.extend((m.data.len() as i32).to_le_bytes());
                body.extend(&m.data);
                n += 1;
            }
            mine.extend(n.to_le_bytes());
            mine.extend(body);
            706
        }
        Op::State(dst) => {
            req.extend(dst.to_le_bytes());
            let d = rs[i].as_ref().unwrap();
            let c = d.endpoint_find_connection(d.system_endpoint, *dst);
            mine.extend((c.is_some() as i32).to_le_bytes());
            if let Some(c) = c {
                let c = &d.conns[c];
                mine.extend((c.flags as i32).to_le_bytes());
                mine.extend(c.flush.to_le_bytes());
                mine.extend(c.flush_ack.to_le_bytes());
                mine.extend(c.arrival_number.to_le_bytes());
                mine.extend(c.arrival_history.to_le_bytes());
                mine.extend(c.tx_seqnum.to_le_bytes());
                mine.extend(c.tx_ack.to_le_bytes());
                mine.extend(c.tx_mask.to_le_bytes());
                mine.extend(c.rx_seqnum.to_le_bytes());
                mine.extend(c.timeout.to_le_bytes());
                mine.extend(c.instance_local.to_le_bytes());
                mine.extend(c.instance_remote.to_le_bytes());
                mine.extend(c.desired_flush_time.to_le_bytes());
                mine.extend(c.time_last_arrival.to_le_bytes());
                mine.extend(c.time_last_flush.to_le_bytes());
                for n in [c.send_queue.len(), c.outstanding.len(), c.delayed.len(), c.arrivals.len()] {
                    mine.extend((n as i32).to_le_bytes());
                }
                for v in c.cstat {
                    mine.extend(v.to_le_bytes());
                }
            }
            707
        }
        Op::Close(dst) => {
            req.extend(dst.to_le_bytes());
            let d = rs[i].as_mut().unwrap();
            let c = d.endpoint_find_connection(d.system_endpoint, *dst);
            if let Some(c) = c {
                d.connection_close(c);
            }
            mine.extend((c.is_some() as i32).to_le_bytes());
            708
        }
        Op::Next => {
            let d = rs[i].as_ref().unwrap();
            let t = d.next_flush_time();
            mine.extend((t.unwrap_or(0) as i32).to_le_bytes());
            mine.extend((t.is_some() as i32).to_le_bytes());
            709
        }
        Op::SendAddr(a, data) => {
            req.extend(a.ip);
            req.extend((a.port as u32).to_le_bytes());
            req.extend((data.len() as u32).to_le_bytes());
            req.extend(*data);
            let d = rs[i].as_mut().unwrap();
            d.send_addr(&[*a], data);
            let sent = std::mem::take(&mut d.outbox);
            packets(&mut mine, &sent);
            710
        }
    };
    (o.call(code, &req), mine)
}

/// The packets in a 704/705/710 reply: (to, bytes).
fn parse_packets(r: &[u8]) -> Vec<(Addr, Vec<u8>)> {
    let n = i32::from_le_bytes(r[0..4].try_into().unwrap()) as usize;
    let mut p = 4;
    let mut v = Vec::new();
    for _ in 0..n {
        let ip = [r[p], r[p + 1], r[p + 2], r[p + 3]];
        let port = u16::from_le_bytes([r[p + 4], r[p + 5]]);
        let len = i32::from_le_bytes(r[p + 6..p + 10].try_into().unwrap()) as usize;
        v.push((Addr { ip, port }, r[p + 10..p + 10 + len].to_vec()));
        p += 10 + len;
    }
    v
}

#[derive(Default)]
struct Check {
    steps: usize,
    bytes: usize,
    /// Messages the decomp delivered, by type.
    delivered: std::collections::BTreeMap<u8, usize>,
    /// Largest message delivered (a reassembled fragment run if > 1300).
    largest: usize,
    /// The connection counters at the end (cstat, both sides summed).
    cstat: [u64; 11],
}

/// Counts a 706 reply's messages.
fn count_received(c: &mut Check, r: &[u8]) {
    let n = i32::from_le_bytes(r[0..4].try_into().unwrap()) as usize;
    let mut p = 4;
    for _ in 0..n {
        let ty = r[p];
        let len = i32::from_le_bytes(r[p + 19..p + 23].try_into().unwrap()) as usize;
        *c.delivered.entry(ty).or_default() += 1;
        c.largest = c.largest.max(len);
        p += 23 + len;
    }
}

impl Check {
    fn eq(&mut self, what: &str, step: usize, theirs: &[u8], mine: &[u8]) {
        self.steps += 1;
        self.bytes += theirs.len();
        if theirs != mine {
            let at = theirs.iter().zip(mine).position(|(a, b)| a != b).unwrap_or(theirs.len().min(mine.len()));
            panic!(
                "step {step}: {what} differs at byte {at} (decomp {} bytes, rust {})\n decomp {:02x?}\n rust   {:02x?}",
                theirs.len(),
                mine.len(),
                &theirs[at.saturating_sub(8)..(at + 24).min(theirs.len())],
                &mine[at.saturating_sub(8)..(at + 24).min(mine.len())]
            );
        }
    }
}

/// A packet in flight: (due time, to instance, bytes behind the sender's
/// address).
type Flight = (u32, usize, Vec<u8>);

/// Two instances, A opening a connection to B, both sending through a bad
/// network. `fault` plants a bug on the Rust side for the negative control.
fn pair(seed: u64, steps: usize, fault: Option<usize>) -> Result<Check, String> {
    let Some(mut o) = Oracle::open() else { return Err("skip".into()) };
    let mut rs: [Option<Drdp<Ieee>>; 4] = [None, None, None, None];
    let mut chk = Check::default();
    let mut rng = Rng(seed);
    let addr = [Addr { ip: [10, 0, 0, 1], port: 5000 }, Addr { ip: [10, 0, 0, 2], port: 6000 }];
    let mut now = 1_000_000u32 + rng.below(1 << 20);
    let mut ports = [0u16; 2];
    for i in 0..2 {
        let (t, m) = run(&mut o, &mut rs, i, &Op::Create(addr[i], now.wrapping_add(i as u32 * 7919)));
        chk.eq("create", 0, &t, &m);
        ports[i] = u16::from_le_bytes([t[0], t[1]]);
    }
    let to_b = address_to_endpoint(addr[1].ip, addr[1].port, PORT_LEADER);
    let a_id = address_to_endpoint(addr[0].ip, addr[0].port, ports[0]);
    let b_id = address_to_endpoint(addr[1].ip, addr[1].port, ports[1]);
    let (t, m) = run(&mut o, &mut rs, 0, &Op::Open(to_b, 1000, 30000, 60000));
    chk.eq("open", 0, &t, &m);
    let mut net: Vec<Flight> = Vec::new();
    let mut state: [[Vec<u8>; 8]; 2] = Default::default();
    for step in 1..=steps {
        // the clock: mostly frames, sometimes a long stall
        now = now.wrapping_add(if rng.chance(1) { 5000 + rng.below(40000) } else { rng.below(60) });
        for i in 0..2 {
            let (t, m) = run(&mut o, &mut rs, i, &Op::Time(now));
            chk.eq("time", step, &t, &m);
        }
        if Some(step) == fault {
            // the planted bug: the Rust side's clock runs a millisecond fast
            rs[0].as_mut().unwrap().set_time(now.wrapping_add(1));
        }
        // sends from each side
        #[allow(clippy::needless_range_loop)]
        for i in 0..2 {
            let dst = if i == 0 { to_b } else { a_id };
            let dst = if i == 0 && step > 1 { b_id } else { dst };
            if !rng.chance(30) {
                continue;
            }
            let (ty, data): (u8, Vec<u8>) = match rng.below(10) {
                0..=3 => {
                    let n = if rng.chance(10) { 1300 + rng.below(3000) } else { 1 + rng.below(200) };
                    (251, (0..n).map(|_| rng.next() as u8).collect())
                }
                4..=7 => {
                    let ch = rng.below(8) as usize;
                    let s = &mut state[i][ch];
                    if s.is_empty() {
                        *s = (0..(20 + rng.below(160))).map(|_| rng.next() as u8).collect();
                    }
                    for _ in 0..rng.below(4) {
                        let k = rng.below(s.len() as u32) as usize;
                        s[k] = rng.next() as u8;
                    }
                    (ch as u8 * 3, s.clone())
                }
                8 => (252, (0..1 + rng.below(100)).map(|_| rng.next() as u8).collect()),
                _ => (249, vec![99, rng.next() as u8]),
            };
            // A's connection is keyed by B's leader id until B answers.
            for d in [dst, to_b] {
                let (t, m) = run(&mut o, &mut rs, i, &Op::Send(d, ty, &data));
                chk.eq("send", step, &t, &m);
                if t[0] == 1 {
                    break;
                }
            }
        }
        // flush, onto the network
        for i in 0..2 {
            let (t, m) = run(&mut o, &mut rs, i, &Op::Flush);
            chk.eq("flush", step, &t, &m);
            for (to, bytes) in parse_packets(&t) {
                if to.ip != addr[1 - i].ip || rng.chance(5) {
                    continue;
                }
                let mut p = addr[i].ip.to_vec();
                p.extend(addr[i].port.to_le_bytes());
                p.extend(&bytes);
                let due = now.wrapping_add(rng.below(120));
                if rng.chance(2) {
                    net.push((due.wrapping_add(rng.below(200)), 1 - i, p.clone()));
                }
                net.push((due, 1 - i, p));
            }
        }
        // deliver what is due, in a random order among the due
        let mut due: Vec<Flight> = Vec::new();
        net.retain(|f| {
            if (f.0.wrapping_sub(now) as i32) <= 0 {
                due.push(f.clone());
                false
            } else {
                true
            }
        });
        while !due.is_empty() {
            let k = rng.below(due.len() as u32) as usize;
            let (_, to, p) = due.remove(k);
            let (t, m) = run(&mut o, &mut rs, to, &Op::Packet(&p));
            chk.eq("packet", step, &t, &m);
        }
        for i in 0..2 {
            let (t, m) = run(&mut o, &mut rs, i, &Op::Receive);
            chk.eq("receive", step, &t, &m);
            count_received(&mut chk, &t);
            let (t, m) = run(&mut o, &mut rs, i, &Op::Next);
            chk.eq("next flush", step, &t, &m);
        }
        if step % 25 == 0 {
            for (i, dst) in [(0, b_id), (0, to_b), (1, a_id)] {
                let (t, m) = run(&mut o, &mut rs, i, &Op::State(dst));
                chk.eq("connection state", step, &t, &m);
            }
        }
    }
    for (i, dst) in [(0, b_id), (1, a_id)] {
        let (t, _) = run(&mut o, &mut rs, i, &Op::State(dst));
        if t[0] == 1 {
            for k in 0..11 {
                chk.cstat[k] += u64::from_le_bytes(t[t.len() - 88 + k * 8..t.len() - 80 + k * 8].try_into().unwrap());
            }
        }
        let (t, m) = run(&mut o, &mut rs, i, &Op::Close(dst));
        chk.eq("close", steps, &t, &m);
        let (t, m) = run(&mut o, &mut rs, i, &Op::Flush);
        chk.eq("flush after close", steps, &t, &m);
    }
    Ok(chk)
}

#[test]
fn two_instances_through_a_bad_network() {
    let mut total = Check::default();
    for seed in 1..=6u64 {
        match pair(seed * 0x9e37_79b9, 2500, None) {
            Err(_) => return,
            Ok(c) => {
                total.steps += c.steps;
                total.bytes += c.bytes;
                for (t, n) in c.delivered {
                    *total.delivered.entry(t).or_default() += n;
                }
                total.largest = total.largest.max(c.largest);
                for k in 0..11 {
                    total.cstat[k] += c.cstat[k];
                }
            }
        }
    }
    eprintln!("{} replies, {} bytes, all equal to the decomp's", total.steps, total.bytes);
    eprintln!("delivered by type: {:?}; largest {} bytes", total.delivered, total.largest);
    eprintln!("cstat (old, nomem, corrupt, crc, out of seq, recv, sent, dup g, g, dup s, s): {:?}", total.cstat);
    // The scenario has to reach the paths it is for.
    assert!(total.largest > 1300, "no fragmented message was reassembled");
    assert!(total.delivered.get(&251).copied().unwrap_or(0) > 100, "too few guaranteed messages");
    assert!(total.delivered.keys().any(|&t| t < 248), "no state channel delivered");
    assert!(total.cstat[7] > 0 && total.cstat[4] > 0, "no duplicates or out-of-sequence segments");
}

#[test]
fn a_planted_bug_is_caught() {
    if Oracle::open().is_none() {
        return;
    }
    let r = std::panic::catch_unwind(|| pair(0x51ed, 400, Some(50)));
    assert!(r.is_err(), "a Rust clock 1 ms off went unnoticed");
}

/// UDP datagrams out of an Ethernet pcap: (seconds, src ip, src port, dst
/// ip, dst port, payload).
type Datagram = (f64, [u8; 4], u16, [u8; 4], u16, Vec<u8>);

fn read_pcap(path: &std::path::Path) -> Option<Vec<Datagram>> {
    let d = std::fs::read(path).ok()?;
    if u32::from_le_bytes(d[0..4].try_into().ok()?) != 0xa1b2c3d4 {
        return None;
    }
    let mut p = 24;
    let mut out = Vec::new();
    while p + 16 <= d.len() {
        let u = |o: usize| u32::from_le_bytes(d[o..o + 4].try_into().unwrap());
        let (ts, tu, cap) = (u(p), u(p + 4), u(p + 8) as usize);
        let b = &d[p + 16..p + 16 + cap];
        p += 16 + cap;
        if b.len() < 42 || b[12..14] != [8, 0] || b[23] != 17 {
            continue;
        }
        let ihl = ((b[14] & 15) * 4) as usize;
        let ip = &b[14..];
        let udp = &ip[ihl..];
        let sport = u16::from_be_bytes([udp[0], udp[1]]);
        let dport = u16::from_be_bytes([udp[2], udp[3]]);
        let len = u16::from_be_bytes([udp[4], udp[5]]) as usize;
        out.push((ts as f64 + tu as f64 / 1e6, ip[12..16].try_into().unwrap(), sport, ip[16..20].try_into().unwrap(), dport, udp[8..len.min(udp.len())].to_vec()));
    }
    Some(out)
}

/// What the application handed DRDP, read back out of one client packet:
/// new reliable messages (fragment runs joined), best-effort ones, and each
/// state channel's full value (undoing the XOR against its reference).
struct AppSends {
    seen: std::collections::BTreeSet<(u8, u16)>,
    fragments: Vec<u8>,
    /// Per channel, every full value by sequence number.
    states: std::collections::BTreeMap<u8, std::collections::BTreeMap<u16, Vec<u8>>>,
}

impl AppSends {
    fn decode(&mut self, payload: &[u8]) -> (u32, Vec<(u8, Vec<u8>)>) {
        use drdp::buffer::{Buffer, rle_decode};
        let mut sends = Vec::new();
        let mut b = Buffer::reader(&payload[..payload.len() - 4]);
        b.u16();
        b.u16();
        let mut instance = 0;
        while b.remaining() > 0 && !b.overflow {
            let h = drdp::read_segment_header(&mut b, 0, 0);
            if h.flags & 0x2000 != 0 {
                instance = h.crc;
            }
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
                    let seq = s.u16();
                    let mut d = vec![0; size];
                    s.read_into(&mut d);
                    if self.seen.insert((ty, seq)) {
                        if ty == 250 {
                            self.fragments.extend(d);
                        } else {
                            let mut whole = std::mem::take(&mut self.fragments);
                            whole.extend(d);
                            sends.push((ty, whole));
                        }
                    }
                } else if ty == 252 {
                    let mut d = vec![0; size];
                    s.read_into(&mut d);
                    sends.push((ty, d));
                } else if ty < 248 {
                    let seq = s.u16();
                    let delta = s.u8();
                    let mut d = vec![0; size];
                    rle_decode(&mut s, &mut d);
                    let hist = self.states.entry(ty).or_default();
                    if delta != 0
                        && let Some(r) = hist.get(&seq.wrapping_sub(delta as u16))
                    {
                        for (x, y) in d.iter_mut().zip(r) {
                            *x ^= y;
                        }
                    }
                    if let std::collections::btree_map::Entry::Vacant(e) = hist.entry(seq) {
                        e.insert(d.clone());
                        sends.push((ty, d));
                    }
                }
            }
        }
        (instance, sends)
    }
}

#[test]
fn a_real_sandstorm_session() {
    let path = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../../pcaps/ps2_sandstorm.pcap");
    let Some(cap) = read_pcap(&path) else {
        eprintln!("SKIP: no capture at {}", path.display());
        return;
    };
    let Some(mut o) = Oracle::open() else { return };
    let server = cap.iter().find(|d| d.2 == 10070).map(|d| (d.1, d.2)).unwrap();
    let client = cap.iter().find(|d| d.4 == 10070).map(|d| (d.1, d.2)).unwrap();
    let mut app = AppSends { seen: Default::default(), fragments: Vec::new(), states: Default::default() };
    // The PS2's instance (its first segment's), which is also its creation
    // clock and so its drdp port.
    let first = cap.iter().find(|d| d.4 == 10070).unwrap();
    let (instance, _) = AppSends { seen: Default::default(), fragments: Vec::new(), states: Default::default() }.decode(&first.5);
    let mut rs: [Option<Drdp<Ieee>>; 4] = [None, None, None, None];
    let mut chk = Check::default();
    let t0 = cap[0].0;
    let at = |t: f64| instance.wrapping_add(((t - t0) * 1000.0) as u32);
    let (t, m) = run(&mut o, &mut rs, 0, &Op::Create(Addr { ip: client.0, port: client.1 }, instance));
    chk.eq("create", 0, &t, &m);
    assert_eq!(u16::from_le_bytes([t[0], t[1]]), u16::from_be_bytes([first.5[0], first.5[1]]).swap_bytes(), "the PS2's drdp port is its instance's low half");
    let to = address_to_endpoint(server.0, server.1, PORT_LEADER);
    let (t, m) = run(&mut o, &mut rs, 0, &Op::Open(to, 1000, 30000, 60000));
    chk.eq("open", 0, &t, &m);
    let mut same = 0;
    let mut ours = 0;
    let mut theirs = 0;
    let mut server_id = to;
    for (step, d) in cap.iter().enumerate() {
        let now = at(d.0);
        let (t, m) = run(&mut o, &mut rs, 0, &Op::Time(now));
        chk.eq("time", step, &t, &m);
        if d.4 == 10070 {
            theirs += 1;
            let (_, sends) = app.decode(&d.5);
            for (ty, data) in sends {
                for dst in [server_id, to] {
                    let (t, m) = run(&mut o, &mut rs, 0, &Op::Send(dst, ty, &data));
                    chk.eq("send", step, &t, &m);
                    if t[0] == 1 {
                        break;
                    }
                }
            }
            let (t, m) = run(&mut o, &mut rs, 0, &Op::Flush);
            chk.eq("flush", step, &t, &m);
            for (_, bytes) in parse_packets(&t) {
                ours += 1;
                same += (bytes == d.5) as usize;
            }
        } else {
            let mut p = d.1.to_vec();
            p.extend(d.2.to_le_bytes());
            p.extend(&d.5);
            let (t, m) = run(&mut o, &mut rs, 0, &Op::Packet(&p));
            chk.eq("packet", step, &t, &m);
            server_id = address_to_endpoint(server.0, server.1, u16::from_le_bytes([d.5[0], d.5[1]]));
            let (t, m) = run(&mut o, &mut rs, 0, &Op::Receive);
            chk.eq("receive", step, &t, &m);
            count_received(&mut chk, &t);
        }
        if step % 50 == 0 {
            for dst in [server_id, to] {
                let (t, m) = run(&mut o, &mut rs, 0, &Op::State(dst));
                chk.eq("connection state", step, &t, &m);
            }
        }
    }
    eprintln!(
        "{} replies, {} bytes equal to the decomp's; delivered {:?}, largest {}; {same} of {ours} packets sent equal to the PS2's own ({theirs} captured)",
        chk.steps, chk.bytes, chk.delivered, chk.largest
    );
    assert!(chk.delivered.values().sum::<usize>() > 100, "the session never got going");
}
