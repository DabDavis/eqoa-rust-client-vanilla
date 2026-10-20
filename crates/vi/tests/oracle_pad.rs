//! `VIPad` against the decomp (eqoa-port's `build/oracle`, ops 50-52), bit
//! for bit under `Ieee`: `ScaleStick` over every byte, and every call of the
//! class over a scripted libpad (the oracle interposes the `scePad*` calls
//! libsupport imports), comparing the return, the pad's bytes, the
//! `VIConsolePad` and the libpad calls made, in order with their arguments.

// `ScaleStick`'s 1/88 as the decomp spells it
#![allow(clippy::excessive_precision)]

mod common;

use common::*;
use std::mem::{offset_of, size_of};
use vi::fpu::{F, Ieee};
use vi::pad::{ConsolePad, Libpad, Pad, scale_stick};

/// The oracle's `PadScript`: what each scePad call answers.
#[repr(C)]
#[derive(Clone, Copy, Debug, Default)]
struct Script {
    open: i32,
    req_state: i32,
    state: i32,
    mode1: i32,
    mode2: i32,
    act_count: i32,
    act0: i32,
    act1: i32,
    align: i32,
    read_ret: i32,
    main_mode: i32,
    act_direct: i32,
    data: [u8; 32],
}

/// The same script on this side, logging as the oracle logs.
struct Scripted {
    s: Script,
    log: Vec<i32>,
}

impl Scripted {
    fn log(&mut self, f: i32, port: i32, slot: i32, a: i32, b: i32) {
        self.log.extend([f, port, slot, a, b]);
    }
}

fn le(p: &[u8]) -> i32 {
    p.iter().enumerate().fold(0, |v, (i, b)| v | (*b as i32) << (8 * i))
}

impl Libpad for Scripted {
    fn port_open(&mut self, port: i32, slot: i32) -> i32 {
        self.log(1, port, slot, 0, 0);
        self.s.open
    }
    fn port_close(&mut self, port: i32, slot: i32) -> i32 {
        self.log(2, port, slot, 0, 0);
        1
    }
    fn get_req_state(&mut self, port: i32, slot: i32) -> i32 {
        self.log(3, port, slot, 0, 0);
        self.s.req_state
    }
    fn get_state(&mut self, port: i32, slot: i32) -> i32 {
        self.log(4, port, slot, 0, 0);
        self.s.state
    }
    fn info_mode(&mut self, port: i32, slot: i32, term: i32, offs: i32) -> i32 {
        self.log(5, port, slot, term, offs);
        match term {
            1 => self.s.mode1,
            2 => self.s.mode2,
            _ => 0,
        }
    }
    fn info_act(&mut self, port: i32, slot: i32, actno: i32, term: i32) -> i32 {
        self.log(6, port, slot, actno, term);
        if actno < 0 {
            self.s.act_count
        } else if actno == 0 {
            self.s.act0
        } else {
            self.s.act1
        }
    }
    fn set_act_align(&mut self, port: i32, slot: i32, a: &[u8; 6]) -> i32 {
        self.log(7, port, slot, le(&a[..4]), le(&a[4..]));
        self.s.align
    }
    fn set_act_direct(&mut self, port: i32, slot: i32, a: &[u8; 6]) -> i32 {
        self.log(8, port, slot, le(&a[..4]), le(&a[4..]));
        self.s.act_direct
    }
    fn set_main_mode(&mut self, port: i32, slot: i32, offs: i32, lock: i32) -> i32 {
        self.log(9, port, slot, offs, lock);
        self.s.main_mode
    }
    fn read(&mut self, port: i32, slot: i32, data: &mut [u8; 32]) -> i32 {
        self.log(10, port, slot, 0, 0);
        *data = self.s.data;
        self.s.read_ret
    }
}

type Cp = ConsolePad<Ieee>;

/// One call: (call, a, b, now).
#[derive(Clone, Copy, Debug)]
struct Call(u32, i32, i32, i64);

/// What a call leaves: the return, the pad, the console pad, the log.
#[derive(Debug, PartialEq)]
struct After {
    ret: i32,
    pad: Vec<u8>,
    cp: Vec<u8>,
    log: Vec<i32>,
}

fn theirs(o: &mut Oracle, c: Call, s: &Script, pad: &Pad, cp: &Cp) -> After {
    let mut p = c.0.to_le_bytes().to_vec();
    p.extend_from_slice(bytes(s));
    p.extend_from_slice(bytes(pad));
    p.extend_from_slice(bytes(cp));
    p.extend_from_slice(&c.1.to_le_bytes());
    p.extend_from_slice(&c.2.to_le_bytes());
    p.extend_from_slice(&c.3.to_le_bytes());
    let r = o.call(52, &p);
    let (np, nc) = (size_of::<Pad>(), size_of::<Cp>());
    let ret = i32::from_le_bytes(r[0..4].try_into().unwrap());
    let pad = r[4..4 + np].to_vec();
    let cp = r[4 + np..4 + np + nc].to_vec();
    let n = i32::from_le_bytes(r[4 + np + nc..8 + np + nc].try_into().unwrap()) as usize;
    let log = r[8 + np + nc..].chunks(4).take(n).map(|w| i32::from_le_bytes(w.try_into().unwrap())).collect();
    After { ret, pad, cp, log }
}

fn ours(c: Call, s: &Script, pad: &mut Pad, cp: &mut Cp) -> After {
    let mut lib = Scripted { s: *s, log: Vec::new() };
    let ret = match c.0 {
        0 => pad.init(&mut lib, c.1, c.2),
        1 => pad.close(&mut lib),
        2 => pad.read(&mut lib, cp),
        3 => pad.process(c.3, &mut lib),
        4 => pad.vibrate_lf(c.1 as u8, c.2),
        5 => pad.vibrate_hf(c.1),
        _ => {
            pad.stop_vibration(&mut lib);
            0
        }
    };
    After { ret, pad: bytes(pad).to_vec(), cp: bytes(cp).to_vec(), log: lib.log }
}

fn script(r: &mut Rng) -> Script {
    let pick = |r: &mut Rng, v: &[i32]| v[(r.next() % v.len() as u64) as usize];
    let mut data = [0u8; 32];
    for b in &mut data {
        *b = r.next() as u8;
    }
    // mostly a successful report; the sticks often at rest or at an edge
    if !r.next().is_multiple_of(5) {
        data[0] = 0;
    }
    for b in &mut data[4..8] {
        *b = match r.next() % 6 {
            0 => 128,
            1 => pick(r, &[0, 87, 88, 167, 168, 255]) as u8,
            _ => r.next() as u8,
        };
    }
    Script {
        open: pick(r, &[1, 1, 1, 0, 2]),
        req_state: pick(r, &[0, 0, 1, 2]),
        state: pick(r, &[6, 6, 6, 2, 0, 1, 7, 5]),
        mode1: pick(r, &[7, 7, 7, 4, 5, 0]),
        mode2: pick(r, &[0, 0, -1, 7, 4]),
        act_count: pick(r, &[2, 2, 0, 1, 3]),
        act0: pick(r, &[0, 0, 1]),
        act1: pick(r, &[1, 1, 0, -1, 2]),
        align: pick(r, &[1, 1, 0, 2]),
        read_ret: pick(r, &[32, 32, 32, 0, -1]),
        main_mode: 1,
        act_direct: 1,
        data,
    }
}

fn pad_state(r: &mut Rng) -> Pad {
    let mut p = Pad::default();
    p.port = (r.next() % 2) as i32;
    p.slot = (r.next() % 2) as i32;
    p.time = (r.next() % 100_000) as i64;
    p.lf_end = if r.next().is_multiple_of(2) { 0 } else { (r.next() % 120_000) as i64 };
    p.hf_end = if r.next().is_multiple_of(2) { 0 } else { (r.next() % 120_000) as i64 };
    p.state = [-1, 0, 2, 2][(r.next() % 4) as usize];
    p.act_aligned = r.next().is_multiple_of(3) as i32;
    p.act = [(r.next() % 2) as u8, r.next() as u8, 0, 0, 0, 0];
    p.dirty = (r.next() % 2) as i32;
    p.mode = (r.next() % 2) as i32;
    p.open = (r.next() % 2) as i32;
    p
}

fn console(r: &mut Rng) -> Cp {
    let mut c = Cp { a: r.next() as i32, ..Default::default() };
    for k in &mut c.keys {
        *k = F::new(r.any());
    }
    c
}

fn call(r: &mut Rng) -> Call {
    let k = r.next() % 10;
    let c = match k {
        0 => 0,
        1 => 1,
        2..=5 => 2,
        6 => 3,
        7 => 4,
        8 => 5,
        _ => 6,
    };
    Call(c, (r.next() % 300) as i32 - 20, (r.next() % 3000) as i32, (r.next() % 130_000) as i64)
}

#[test]
fn pad_layout_matches() {
    let Some(mut o) = Oracle::open() else { return };
    let r = o.call(50, &[]);
    let v: Vec<usize> = r.chunks(4).map(|c| u32::from_le_bytes(c.try_into().unwrap()) as usize).collect();
    let ours = [
        0,
        size_of::<Pad>(),
        size_of::<Cp>(),
        offset_of!(Pad, time),
        offset_of!(Pad, lf_end),
        offset_of!(Pad, hf_end),
        offset_of!(Pad, state),
        offset_of!(Pad, act_aligned),
        offset_of!(Pad, act),
        offset_of!(Pad, dirty),
        offset_of!(Pad, mode),
        offset_of!(Pad, open),
        offset_of!(Pad, data),
        offset_of!(Cp, keys),
        offset_of!(Cp, keys) + 19 * 4,
    ];
    assert_eq!(v, ours, "VIPad and VIConsolePad: sizes and offsets");
}

#[test]
fn scale_stick_matches_over_every_byte() {
    let Some(mut o) = Oracle::open() else { return };
    let r = o.call(51, &[]);
    let theirs: Vec<u32> = r[4..].chunks(4).map(|c| u32::from_le_bytes(c.try_into().unwrap())).collect();
    assert_eq!(theirs.len(), 256);
    let ours: Vec<u32> = (0..=255u8).map(|v| scale_stick::<Ieee>(v).bits()).collect();
    assert_eq!(ours, theirs);
    // planted: the dead zone's upper edge a step late, and the scale one
    // ulp off; both must be seen
    let late: Vec<u32> = (0..=255u8).map(|v| if v == 168 { 0 } else { scale_stick::<Ieee>(v).bits() }).collect();
    assert_ne!(late, theirs, "a dead zone one step wider would pass");
    let ulp: Vec<u32> = (0..=255u8)
        .map(|v| if v < 88 { (F::<Ieee>::from_int(88 - v as i32) * F::new(f32::from_bits((-0.0113636364f32).to_bits() + 1))).bits() } else { scale_stick::<Ieee>(v).bits() })
        .collect();
    assert_ne!(ulp, theirs, "a scale one ulp off would pass");
}

/// Single calls from random states, then whole runs of calls on one pad,
/// the script changing between them (a pad found, the handshake, rumble,
/// unplugged, found again).
#[test]
fn every_call_matches_over_scripted_libpad() {
    let Some(mut o) = Oracle::open() else { return };
    let mut r = Rng(0x9AD0_0001);
    // how often each libpad call and each interesting branch came up
    let mut seen = [0usize; 11];
    let (mut unpacked, mut aligned, mut busy, mut active_teardown, mut expired) = (0, 0, 0, 0, 0);
    let mut check = |o: &mut Oracle, c: Call, s: &Script, pad: &mut Pad, cp: &mut Cp, what: &str| {
        let t = theirs(o, c, s, pad, cp);
        let before_active = pad.active();
        let u = ours(c, s, pad, cp);
        assert_eq!(u, t, "{what}: {c:?} with {s:?}");
        for e in t.log.chunks(5) {
            seen[e[0] as usize] += 1;
        }
        if c.0 == 2 && t.ret == 1 {
            unpacked += 1;
        }
        if t.log.chunks(5).any(|e| e[0] == 7) {
            aligned += 1;
        }
        if c.0 == 2 && t.log.first() == Some(&3) && s.req_state == 2 {
            busy += 1;
        }
        if c.0 == 2 && before_active && t.log.chunks(5).any(|e| e[0] == 8) {
            active_teardown += 1;
        }
        if c.0 == 3 && t.log.chunks(5).any(|e| e[0] == 8) {
            expired += 1;
        }
    };
    for _ in 0..20_000 {
        let s = script(&mut r);
        let (mut pad, mut cp) = (pad_state(&mut r), console(&mut r));
        let c = call(&mut r);
        check(&mut o, c, &s, &mut pad, &mut cp, "one call");
    }
    for run in 0..400 {
        let mut s = script(&mut r);
        let (mut pad, mut cp) = (Pad::default(), console(&mut r));
        s.open = 1;
        check(&mut o, Call(0, 0, 0, 0), &s, &mut pad, &mut cp, "init");
        let mut now = 0i64;
        for step in 0..60 {
            // the pad not found for a while, then a DualShock with its
            // motors, now and then unplugged again
            let mut s2 = script(&mut r);
            if step < 3 || r.next().is_multiple_of(9) {
                s2.state = [0, 1][(r.next() % 2) as usize];
            } else if !r.next().is_multiple_of(3) {
                (s2.state, s2.mode1, s2.mode2, s2.act_count, s2.act0, s2.act1, s2.align) = (6, 7, 0, 2, 0, 1, 1);
                s2.req_state = [0, 0, 0, 2, 1][(r.next() % 5) as usize];
                s2.read_ret = 32;
                s2.data[0] = 0;
            }
            now += (r.next() % 40) as i64;
            let c = match r.next() % 8 {
                0..=3 => Call(2, 0, 0, 0),
                4 => Call(3, 0, 0, now),
                5 => Call(4, (r.next() % 256) as i32, (r.next() % 400) as i32, 0),
                6 => Call(5, (r.next() % 400) as i32, 0, 0),
                _ => if r.next().is_multiple_of(4) { Call(6, 0, 0, 0) } else { Call(3, 0, 0, now) },
            };
            check(&mut o, c, &s2, &mut pad, &mut cp, &format!("run {run} step {step}"));
        }
        s.open = 1;
        check(&mut o, Call(1, 0, 0, 0), &s, &mut pad, &mut cp, "close");
    }
    // every libpad call, and each branch the checks rest on, was reached
    for (f, n) in seen.iter().enumerate().skip(1) {
        assert!(*n > 50, "libpad call {f} made only {n} times");
    }
    for (what, n) in [("unpacked", unpacked), ("aligned", aligned), ("busy", busy), ("active teardown", active_teardown), ("expired", expired)] {
        assert!(n > 20, "{what}: only {n}");
    }
}

/// The checks above would see a button pair crossed in the unpack, a
/// handshake step dropped from the log, or a motor's deadline off by one.
#[test]
fn planted_mistakes_are_seen() {
    let Some(mut o) = Oracle::open() else { return };
    let mut r = Rng(0x9AD0_0002);
    let (mut crossed, mut dropped, mut late) = (0, 0, 0);
    for _ in 0..3000 {
        let mut s = script(&mut r);
        (s.state, s.mode1, s.mode2, s.read_ret, s.data[0]) = (6, 7, 0, 32, 0);
        let (mut pad, mut cp) = (pad_state(&mut r), console(&mut r));
        pad.act_aligned = 0;
        let t = theirs(&mut o, Call(2, 0, 0, 0), &s, &pad, &cp);
        let mut u = ours(Call(2, 0, 0, 0), &s, &mut pad.clone(), &mut cp);
        // L1 and R1 swapped
        let (l1, r1) = (8 + 12 * 4, 8 + 14 * 4);
        let a: [u8; 4] = u.cp[l1 - 4..l1].try_into().unwrap();
        let b: [u8; 4] = u.cp[r1 - 4..r1].try_into().unwrap();
        u.cp[l1 - 4..l1].copy_from_slice(&b);
        u.cp[r1 - 4..r1].copy_from_slice(&a);
        crossed += (u != t) as usize;
        // the handshake's InfoAct(0, 3) left out
        let mut u = ours(Call(2, 0, 0, 0), &s, &mut pad.clone(), &mut cp.clone());
        if let Some(k) = u.log.chunks(5).position(|e| e[0] == 6 && e[3] == 0) {
            u.log.drain(k * 5..k * 5 + 5);
        }
        dropped += (u != t) as usize;
        // a deadline compared with <= instead of <: at 500 it behaves as <
        // does against 499
        let mut q = pad;
        q.state = 2;
        q.lf_end = 500;
        let t = theirs(&mut o, Call(3, 0, 0, 500), &s, &q, &cp);
        let mut q2 = q;
        q2.lf_end = 499;
        let u = ours(Call(3, 0, 0, 500), &s, &mut q2, &mut cp.clone());
        late += (u.log != t.log) as usize;
    }
    assert!(crossed > 1000, "L1/R1 crossed seen {crossed} of 3000");
    assert!(dropped > 100, "a dropped handshake call seen {dropped} of 3000");
    assert!(late > 100, "an expiry a tick late seen {late} of 3000");
}
