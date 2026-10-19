//! SNDDRV, the IOP's sound driver: `driver.cpp` and `bgm.cpp` line by line,
//! over the IOP of `iop.rs`.
//!
//! On the IOP it is three threads: the RPC server that runs each packet of
//! commands from the EE, the XM thread a 100 Hz timer wakes, and the BGM
//! thread that runs once a vblank. Here they are state machines on one clock,
//! the SPU2's sample count, scheduled exactly as eqoa-port's sound oracle
//! schedules the decomp's own threads (`test/oracle/oracle_snd.cpp`):
//! - the timer fires every 480 samples (10 ms), the first 480 after
//!   `StartHardTimer`, and the XM thread runs its tick at once;
//! - a vblank is every 800 samples from sample 0, and readies the threads
//!   waiting for one: the BGM thread (priority 29) runs before the RPC server
//!   (30);
//! - a timer and a vblank on one sample: the timer first.
//!
//! An RPC that waits (`SdStopBgm` waits for the BGM thread to see its stop)
//! keeps its place in the packet as a `Cont`, and goes on at the vblank.

use crate::iop::{Iop, DEFAULT_IOP_BUF, DEFAULT_XM_POOL, XM_POOL_SIZE};
use crate::xm::{self, XmSlot};

pub const VBLANK: u64 = 800;
/// SNDDRV's timer: 10,000 µs, as samples
const TIMER: u64 = 480;
/// core 1, voices 22 and 23
const BGM_LEFT: u16 = 0x2d;
const BGM_RIGHT: u16 = 0x2f;
const BGM_HALF: usize = 0x10000;

/// The disc, as CDSTREAM gives it to SNDDRV: a file's first sector and size
/// by its ISO 9660 name (`\BGM\THEME.BGM;1`), and sectors by number.
pub trait Disc: Send {
    fn search(&mut self, name: &str) -> Option<(u32, u32)>;
    fn read(&mut self, lsn: u32, sectors: u32, buf: &mut [u8]) -> bool;
}

/// No disc: every BGM fails to open.
pub struct NoDisc;
impl Disc for NoDisc {
    fn search(&mut self, _: &str) -> Option<(u32, u32)> {
        None
    }
    fn read(&mut self, _: u32, _: u32, _: &mut [u8]) -> bool {
        false
    }
}

fn sd_voice(ch: i32) -> u16 {
    (if ch < 24 { ch << 1 } else { ((ch - 24) << 1) | 1 }) as u16
}

fn sd_clamp(x: i32) -> i32 {
    x.clamp(0, 0x3fff)
}

/// Where the BGM thread waits: at the top of its loop, or in one of state 3's
/// two transfer loops.
#[derive(Clone, Copy, PartialEq, Debug)]
enum BgmPc {
    Top,
    Fill0,
    Fill1,
}

#[derive(Default)]
struct Bgm {
    /// 3 start, 6 stopped, 7 streaming
    state: i32,
    /// the half the left voice is playing
    half: i32,
    /// the half being uploaded
    xfer_half: i32,
    /// a stop request for the thread
    stop: i32,
    quit: i32,
    /// the thread, while it lives
    thread: Option<BgmPc>,
    /// two 128 KB IOP buffers, one disc read each
    buf: [Vec<u8>; 2],
    spu_left: u32,
    spu_right: u32,
    /// the open file (0), or -1
    fd: i32,
    /// 0 idle, 1 left half going, 2 right half going
    xfer: i32,
    volume: u32,
    cd_lsn: u32,
    cd_size: u32,
    cd_sector: u32,
}

/// What a waiting RPC does when it goes on.
#[derive(Clone, Debug)]
enum Cont {
    /// `SdStopBgm`, in `while (gBgmStop) WaitVblankEnd();`, then `after`
    Stop(After),
    /// `SdPlayBgm`, in `while (!BgmTransferDone()) WaitVblankEnd();`
    PlayXfer { name: String, spu: u32 },
    /// `SdFreeBgmSystem`, polling the BGM thread's status
    FreePoll,
}

#[derive(Clone, Debug)]
enum After {
    Nothing,
    PlayBgm { name: String, spu: u32 },
    FreeBgm,
}

struct Rpc {
    /// gRpcBuf as words: the 512 bytes of the call, zero past them
    words: Vec<i32>,
    /// the command count, the next command, and the next word
    n: i32,
    i: i32,
    p: usize,
    cont: Option<Cont>,
}

pub struct Driver {
    pub io: Iop,
    disc: Box<dyn Disc>,
    /// samples mixed
    pub now: u64,
    timer_running: bool,
    timer_next: u64,
    pub xm_ticks: i32,
    pub slots: [XmSlot; 8],
    bgm: Bgm,
    rpc: Option<Rpc>,
    status: [u32; 128],
    answered: bool,
}

impl Driver {
    pub fn new(disc: Box<dyn Disc>) -> Driver {
        Driver::with_addresses(disc, DEFAULT_IOP_BUF, DEFAULT_XM_POOL)
    }

    /// With SNDDRV's two EE-visible buffers at these IOP addresses.
    pub fn with_addresses(disc: Box<dyn Disc>, iop_buf: u32, xm_pool: u32) -> Driver {
        Driver {
            io: Iop::new(iop_buf, xm_pool),
            disc,
            now: 0,
            timer_running: false,
            timer_next: 0,
            xm_ticks: 0,
            slots: Default::default(),
            bgm: Bgm { fd: -1, ..Default::default() },
            rpc: None,
            status: [0; 128],
            answered: false,
        }
    }

    // ---- time ---------------------------------------------------------------------

    fn mix(&mut self, n: u64, out: &mut Vec<i16>) {
        let at = out.len();
        out.resize(at + 2 * n as usize, 0);
        self.io.spu.mix(&mut out[at..]);
        self.now += n;
    }

    /// The next sample something happens on: the timer or a vblank.
    pub fn next_event(&self) -> u64 {
        let vb = (self.now / VBLANK + 1) * VBLANK;
        if self.timer_running && self.timer_next < vb { self.timer_next } else { vb }
    }

    /// Mix up to the next event or `until`, whichever comes first, then make
    /// the event happen.
    pub fn step(&mut self, until: u64, out: &mut Vec<i16>) {
        let vb = (self.now / VBLANK + 1) * VBLANK;
        let mut next = until;
        if self.timer_running && self.timer_next < next {
            next = self.timer_next;
        }
        if vb < next {
            next = vb;
        }
        if next > self.now {
            self.mix(next - self.now, out);
        }
        if self.timer_running && self.now == self.timer_next {
            self.xm_thread();
            self.timer_next += TIMER;
        }
        if self.now == vb {
            self.vblank();
        }
    }

    /// Run `frames` samples.
    pub fn run(&mut self, frames: u64, out: &mut Vec<i16>) {
        let until = self.now + frames;
        while self.now < until {
            self.step(until, out);
        }
    }

    fn vblank(&mut self) {
        // priority 29 before 30
        if let Some(pc) = self.bgm.thread {
            self.bgm_thread(pc);
        }
        if let Some(rpc) = &mut self.rpc
            && let Some(cont) = rpc.cont.take()
        {
            self.resume(cont);
        }
    }

    // ---- the RPC server --------------------------------------------------------------

    /// An EE call: the packet (`fno` 0x80f0) runs on the RPC server, and its
    /// answer is the status block. A call that waits lets time run, mixing into
    /// `out`, until it answers.
    pub fn call(&mut self, fno: u32, packet: &[u32; 128], out: &mut Vec<i16>) -> [u32; 128] {
        self.begin_call(fno, packet);
        while !self.answered {
            self.step(u64::MAX, out);
        }
        self.status
    }

    /// Start an EE call; `answer` has the status once it is done.
    pub fn begin_call(&mut self, fno: u32, packet: &[u32; 128]) {
        self.answered = false;
        let mut words = vec![0i32; 0x810 / 4];
        for (w, &p) in words.iter_mut().zip(packet.iter()) {
            *w = p as i32;
        }
        if fno & 0xfff0 == 0x80f0 {
            let n = words[0];
            self.rpc = Some(Rpc { words, n, i: 0, p: 1, cont: None });
            self.run_commands();
        } else {
            self.finish_call();
        }
    }

    pub fn answer(&self) -> Option<[u32; 128]> {
        self.answered.then_some(self.status)
    }

    pub fn busy(&self) -> bool {
        !self.answered && self.rpc.is_some()
    }

    fn word(&self, k: usize) -> i32 {
        self.rpc.as_ref().and_then(|r| r.words.get(k).copied()).unwrap_or(0)
    }

    /// `SdCommand`'s loop, from where the packet stands.
    fn run_commands(&mut self) {
        loop {
            let (i, n, p) = {
                let r = self.rpc.as_ref().unwrap();
                (r.i, r.n, r.p)
            };
            if i >= n {
                break;
            }
            let id = self.word(p);
            let argc = self.word(p + 1);
            let a = p + 2;
            {
                let r = self.rpc.as_mut().unwrap();
                r.i += 1;
                r.p = (a as i64 + argc as i64).max(0) as usize;
            }
            let arg = |k: usize| self.word(a + k);
            let args: [i32; 9] = std::array::from_fn(arg);
            let flow = match id {
                0 => {
                    self.sd_init_spu();
                    None
                }
                12 => self.sd_free_spu(),
                1 => {
                    self.sd_adpcm_transfer(args[0], args[1] as u32);
                    None
                }
                2 => {
                    self.sd_play_wave(args[0] as u32, args[1] as u32, args[2] as u32, args[3] as u32, args[4], args[5], args[6]);
                    None
                }
                4 => {
                    self.sd_stop_voices(args[0] as u32, args[1] as u32);
                    None
                }
                5 => {
                    self.sd_set_pitch(args[0], args[1], args[2]);
                    None
                }
                6 => {
                    self.sd_set_volume(args[0], args[1], args[2], args[3]);
                    None
                }
                7 => {
                    xm::relocate(&mut self.io, args[0] as u32, args[1] as u32);
                    None
                }
                8 => {
                    self.sd_xm_play(args[0] as u32, args[1] as u32, args[2] as u32, args[3] as u32, args[4], args[5], args[6]);
                    None
                }
                10 => {
                    self.sd_xm_set_volume(args[0] as u32, args[1] as u32, args[2], args[3]);
                    None
                }
                11 => {
                    self.sd_xm_set_pitch(args[0] as u32, args[1] as u32, args[2]);
                    None
                }
                9 => {
                    self.sd_xm_stop(args[0] as u32, args[1] as u32);
                    None
                }
                99 => {
                    self.sd_test_tone(args[0] as u32, args[1] as u32);
                    None
                }
                13 => {
                    // the name is the packet's words from the first argument, as a C string
                    let mut name = Vec::new();
                    let mut k = 0;
                    loop {
                        let b = (self.word(a + k / 4) as u32).to_le_bytes()[k % 4];
                        if b == 0 {
                            break;
                        }
                        name.push(b);
                        k += 1;
                    }
                    self.sd_play_bgm(String::from_utf8_lossy(&name).into_owned(), args[8] as u32)
                }
                14 => self.sd_stop_bgm(After::Nothing),
                15 => {
                    self.sd_set_bgm_volume(args[0] as u32);
                    None
                }
                _ => None,
            };
            if let Some(cont) = flow {
                self.rpc.as_mut().unwrap().cont = Some(cont);
                return;
            }
        }
        self.finish_call();
    }

    /// A waiting command goes on at a vblank.
    fn resume(&mut self, cont: Cont) {
        let flow = match cont {
            Cont::Stop(after) => {
                if self.bgm.stop != 0 {
                    Some(Cont::Stop(after))
                } else {
                    self.sd_stop_bgm_rest();
                    self.after_stop(after)
                }
            }
            Cont::PlayXfer { name, spu } => self.sd_play_bgm_wait(name, spu),
            Cont::FreePoll => self.sd_free_bgm_poll(),
        };
        match flow {
            Some(cont) => self.rpc.as_mut().unwrap().cont = Some(cont),
            None => self.run_commands(),
        }
    }

    /// `SdRpc`'s reply: the status block every call answers with.
    fn finish_call(&mut self) {
        let s = &mut self.status;
        s[0] = self.io.buf_addr;
        s[1] = 0;
        s[2] = self.io.pool_addr;
        s[3] = XM_POOL_SIZE as u32;
        s[4] = 0;
        s[5] = 0;
        for i in 0..24u16 {
            if self.io.sd_get_param((i << 1) | 0x500) != 0 {
                s[4] |= 1 << i;
            }
            if self.io.sd_get_param((i << 1) | 0x501) != 0 {
                s[5] |= 1 << i;
            }
        }
        s[6] = (self.bgm.fd >= 0) as u32;
        for i in 0..8 {
            s[i * 2 + 8] = self.slots[i].lo;
            s[i * 2 + 9] = self.slots[i].hi;
        }
        self.rpc = None;
        self.answered = true;
    }

    // ---- the XM thread -----------------------------------------------------------

    /// Each wake adds tempo * pitch / 4096 to a slot's accumulator and every
    /// 250 of it is one XM tick. A module that ends stops its slot.
    fn xm_thread(&mut self) {
        self.xm_ticks += 1;
        for i in 0..8 {
            if self.slots[i].active != 0 {
                let s = &mut self.slots[i];
                s.tick = s.tick.wrapping_add(s.player.tempo().wrapping_mul(s.player.pitch()) >> 12);
                while self.slots[i].tick >= 250 {
                    self.slots[i].tick -= 250;
                    let mut player = std::mem::take(&mut self.slots[i].player);
                    let more = player.process(&mut self.io);
                    self.slots[i].player = player;
                    if !more {
                        let (lo, hi) = (self.slots[i].lo, self.slots[i].hi);
                        self.sd_xm_stop(lo, hi);
                        break;
                    }
                }
            }
        }
    }

    // ---- the command handlers ------------------------------------------------------

    fn sd_init_spu(&mut self) {
        self.io.sd_init();
        self.io.sd_set_switch(0x1600, 0xffffff);
        self.io.sd_set_switch(0x1601, 0xffffff);
        for _ in 0..4 {
            self.io.sd_set_param(0xffff, 0);
        }
        self.io.sd_set_param(0x980, 0x3fff);
        self.io.sd_set_param(0xa80, 0x3fff);
        self.io.sd_set_param(0x981, 0x3fff);
        self.io.sd_set_param(0xa81, 0x3fff);
        for e in [0xf80, 0x1080, 0xf81, 0x1081, 0xb80, 0xc80, 0xb81, 0xc81] {
            self.io.sd_set_param(e, 0);
        }
        self.timer_running = true;
        self.timer_next = self.now + TIMER;
        self.sd_init_bgm_system();
    }

    fn sd_free_spu(&mut self) -> Option<Cont> {
        self.sd_free_bgm_system()
    }

    fn sd_free_spu_rest(&mut self) {
        self.timer_running = false;
        self.io.sd_init();
        self.io.sd_set_param(0x980, 0x3fff);
        self.io.sd_set_param(0xa80, 0x3fff);
        self.io.sd_set_param(0x981, 0x3fff);
        self.io.sd_set_param(0xa81, 0x3fff);
    }

    /// The EE has already DMA'd the block into gIopBuf; this moves it on to
    /// SPU RAM.
    fn sd_adpcm_transfer(&mut self, size: i32, spu_addr: u32) {
        let n = (size.max(0) as usize).min(self.io.buf.len());
        let data = self.io.buf[..n].to_vec();
        self.io.spu.write(spu_addr, &data);
    }

    /// A wave is one voice, or two for stereo (`second`'s data starts at `addr2`).
    #[allow(clippy::too_many_arguments)]
    fn sd_play_wave(&mut self, first: u32, addr: u32, second: u32, addr2: u32, pitch: i32, volume: i32, pan: i32) {
        let mut lo = 0u32;
        let mut hi = 0u32;
        let voice = |ch: u32| -> u32 { if ch < 24 { ch << 1 } else { (ch.wrapping_sub(24) << 1) | 1 } };
        let e1 = voice(first);
        let e2 = voice(second);
        self.sd_set_pitch(first as i32, second as i32, pitch);
        self.sd_set_volume(first as i32, second as i32, volume, pan);
        self.io.sd_set_addr((e1 | 0x2040) as u16, addr);
        if first < 24 {
            lo = 1u32.wrapping_shl(first);
        } else {
            hi = 1u32.wrapping_shl(first.wrapping_sub(24));
        }
        if addr2 != 0 {
            self.io.sd_set_addr((e2 | 0x2040) as u16, addr2);
            if second < 24 {
                lo |= 1u32.wrapping_shl(second);
            } else {
                hi |= 1u32.wrapping_shl(second.wrapping_sub(24));
            }
        }
        self.io.sd_set_switch(0x1500, lo);
        self.io.sd_set_switch(0x1501, hi);
    }

    fn sd_stop_voices(&mut self, lo: u32, hi: u32) {
        self.io.sd_set_switch(0x1600, lo);
        self.io.sd_set_switch(0x1601, hi);
    }

    fn sd_set_pitch(&mut self, first: i32, second: i32, pitch: i32) {
        self.io.sd_set_param(sd_voice(first) | 0x200, pitch as u16);
        if second != -1 {
            self.io.sd_set_param(sd_voice(second) | 0x200, pitch as u16);
        }
    }

    /// A linear pan law on 0..0x3fff: mono puts all of `pan` on one voice's two
    /// sides; stereo gives each voice of the pair one side of it.
    fn sd_set_volume(&mut self, first: i32, second: i32, volume: i32, pan: i32) {
        let (l1, r1, l2, r2);
        if second == -1 {
            l1 = volume.wrapping_sub(volume.wrapping_mul(pan) >> 14);
            r1 = volume.wrapping_add(volume.wrapping_mul(pan) >> 14);
            l2 = 0;
            r2 = 0;
        } else {
            l1 = volume.wrapping_sub(volume.wrapping_mul(pan) >> 14);
            r1 = volume.wrapping_mul(pan) >> 14;
            l2 = volume.wrapping_neg().wrapping_mul(pan) >> 14;
            r2 = volume.wrapping_add(volume.wrapping_mul(pan) >> 14);
        }
        let e1 = sd_voice(first);
        self.io.sd_set_param(e1, sd_clamp(l1) as u16);
        self.io.sd_set_param(e1 | 0x100, sd_clamp(r1) as u16);
        if second != -1 {
            let e2 = sd_voice(second);
            self.io.sd_set_param(e2, sd_clamp(l2) as u16);
            self.io.sd_set_param(e2 | 0x100, sd_clamp(r2) as u16);
        }
    }

    #[allow(clippy::too_many_arguments)]
    fn sd_xm_play(&mut self, lo: u32, hi: u32, xm_addr: u32, repeat: u32, pitch: i32, volume: i32, pan: i32) {
        for i in 0..8 {
            if self.slots[i].active == 0 {
                let s = &mut self.slots[i];
                s.lo = lo;
                s.hi = hi;
                s.tick = 0;
                s.active = xm_addr;
                s.player.play(&self.io, lo, xm_addr, hi);
                s.player.set_repeat((repeat & 1) as i32);
                s.player.set_volume(volume);
                s.player.set_pan(pan);
                s.player.set_pitch(pitch);
                break;
            }
        }
    }

    fn find_xm(&self, lo: u32, hi: u32) -> Option<usize> {
        (0..8).find(|&i| self.slots[i].active != 0 && self.slots[i].lo == lo && self.slots[i].hi == hi)
    }

    fn sd_xm_set_volume(&mut self, lo: u32, hi: u32, volume: i32, pan: i32) {
        if let Some(i) = self.find_xm(lo, hi) {
            self.slots[i].player.set_volume(volume);
            self.slots[i].player.set_pan(pan);
        }
    }

    fn sd_xm_set_pitch(&mut self, lo: u32, hi: u32, pitch: i32) {
        if let Some(i) = self.find_xm(lo, hi) {
            self.slots[i].player.set_pitch(pitch);
        }
    }

    fn sd_xm_stop(&mut self, lo: u32, hi: u32) {
        if let Some(i) = self.find_xm(lo, hi) {
            let s = &mut self.slots[i];
            s.player.stop();
            s.lo = 0;
            s.hi = 0;
            s.active = 0;
        }
    }

    /// Command 99, never sent by retail: a stereo pair on core 0 voices 4
    /// and 5, the two halves of one buffer, hard left and hard right.
    fn sd_test_tone(&mut self, spu_addr: u32, size: u32) {
        let (left, right) = (4u16, 5u16);
        self.io.sd_set_param(left << 1, 0x3fff);
        self.io.sd_set_param((left << 1) | 0x100, 0);
        self.io.sd_set_param((left << 1) | 0x200, 0xe80);
        self.io.sd_set_addr((left << 1) | 0x2040, spu_addr);
        self.io.sd_set_param(right << 1, 0);
        self.io.sd_set_param((right << 1) | 0x100, 0x3fff);
        self.io.sd_set_param((right << 1) | 0x200, 0xe80);
        self.io.sd_set_addr((right << 1) | 0x2040, spu_addr.wrapping_add(size >> 1));
        self.io.sd_set_switch(0x1500, (1 << left) | (1 << right));
    }

    // ---- the BGM: bgm.cpp ---------------------------------------------------------

    fn sd_init_bgm_system(&mut self) {
        let b = &mut self.bgm;
        b.quit = 0;
        b.thread = Some(BgmPc::Top);
        b.buf = [vec![0; 2 * BGM_HALF], vec![0; 2 * BGM_HALF]];
        b.fd = -1;
        b.volume = 0x3fff;
        b.xfer = 0;
        b.xfer_half = 0;
        b.stop = 0;
    }

    fn sd_free_bgm_system(&mut self) -> Option<Cont> {
        self.bgm.quit = 1;
        if self.bgm.fd >= 0 {
            return self.sd_stop_bgm(After::FreeBgm);
        }
        self.sd_free_bgm_poll()
    }

    fn sd_free_bgm_poll(&mut self) -> Option<Cont> {
        if self.bgm.thread.is_some() {
            return Some(Cont::FreePoll);
        }
        self.bgm.buf = [Vec::new(), Vec::new()];
        self.sd_free_spu_rest();
        None
    }

    fn after_stop(&mut self, after: After) -> Option<Cont> {
        match after {
            After::Nothing => None,
            After::PlayBgm { name, spu } => self.sd_play_bgm_wait(name, spu),
            After::FreeBgm => self.sd_free_bgm_poll(),
        }
    }

    fn sd_play_bgm(&mut self, name: String, spu: u32) -> Option<Cont> {
        self.bgm.stop = 0;
        if self.bgm.fd != -1 {
            return self.sd_stop_bgm(After::PlayBgm { name, spu });
        }
        self.sd_play_bgm_wait(name, spu)
    }

    fn sd_play_bgm_wait(&mut self, name: String, spu: u32) -> Option<Cont> {
        if !self.bgm_transfer_done() {
            return Some(Cont::PlayXfer { name, spu });
        }
        let b = &mut self.bgm;
        b.spu_left = spu;
        b.spu_right = spu.wrapping_add(0x20000);
        for buf in &mut b.buf {
            buf.fill(0);
        }
        self.bgm.fd = self.cd_open(&name);
        if self.bgm.fd < 0 {
            return None;
        }
        self.bgm.state = 3;
        None
    }

    fn sd_stop_bgm(&mut self, after: After) -> Option<Cont> {
        self.bgm.state = 6;
        self.bgm.stop = 1;
        Some(Cont::Stop(after))
    }

    fn sd_stop_bgm_rest(&mut self) {
        self.io.sd_set_switch(0x1601, 0xc00000);
        if self.bgm.fd >= 0 {
            self.bgm.fd = -1;
        }
    }

    fn sd_set_bgm_volume(&mut self, volume: u32) {
        self.bgm.volume = volume;
        self.io.sd_set_param(BGM_LEFT, volume as u16);
        self.io.sd_set_param(BGM_LEFT | 0x100, 0);
        self.io.sd_set_param(BGM_RIGHT, 0);
        self.io.sd_set_param(BGM_RIGHT | 0x100, volume as u16);
    }

    fn bgm_close(&mut self) {
        if self.bgm.fd >= 0 {
            self.bgm.fd = -1;
        }
    }

    /// One vblank of the BGM thread, from where it waits to where it waits next.
    fn bgm_thread(&mut self, pc: BgmPc) {
        let next = match pc {
            BgmPc::Top => self.bgm_top(),
            BgmPc::Fill0 => self.bgm_fill0(),
            BgmPc::Fill1 => self.bgm_fill1(),
        };
        self.bgm.thread = next;
    }

    fn bgm_top(&mut self) -> Option<BgmPc> {
        if self.bgm.stop != 0 {
            self.bgm.state = 6;
            self.bgm.stop = 0;
        }
        if self.bgm.quit != 0 {
            return None;
        }
        if self.bgm.fd == -1 {
            return Some(BgmPc::Top);
        }
        match self.bgm.state {
            3 => {
                self.bgm.half = 0;
                self.bgm.xfer = 0;
                if self.bgm_read(0) < 0 || self.bgm_read(1) < 0 {
                    self.bgm_close();
                    self.bgm.state = 6;
                    return Some(BgmPc::Top);
                }
                self.bgm_transfer(0);
                self.bgm_fill0()
            }
            7 => {
                if self.bgm_transfer_done() {
                    let addr = self.io.sd_get_addr(BGM_LEFT | 0x2240) as i32;
                    let mid = self.bgm.spu_left.wrapping_add(BGM_HALF as u32) as i32;
                    let mut refill = false;
                    if self.bgm.half == 0 && addr >= mid {
                        self.bgm.half = 1;
                        refill = true;
                    } else if self.bgm.half == 1 && addr < mid {
                        self.bgm.half = 0;
                        refill = true;
                    }
                    if refill {
                        if self.bgm_read(1 - self.bgm.half) < 0 {
                            self.bgm.state = 6;
                            self.io.sd_set_switch(0x1601, 0xc00000);
                            self.bgm_close();
                            return Some(BgmPc::Top);
                        }
                        self.bgm_transfer(1 - self.bgm.half);
                    }
                }
                Some(BgmPc::Top)
            }
            _ => Some(BgmPc::Top),
        }
    }

    /// `while (!BgmTransferDone() && gBgmState != 6) WaitVblankEnd();` after
    /// the first half
    fn bgm_fill0(&mut self) -> Option<BgmPc> {
        if !self.bgm_transfer_done() && self.bgm.state != 6 {
            return Some(BgmPc::Fill0);
        }
        if self.bgm.state == 6 {
            return Some(BgmPc::Top);
        }
        self.bgm_transfer(1);
        self.bgm_fill1()
    }

    /// ... and after the second, then the voices start.
    fn bgm_fill1(&mut self) -> Option<BgmPc> {
        if !self.bgm_transfer_done() && self.bgm.state != 6 {
            return Some(BgmPc::Fill1);
        }
        if self.bgm.state == 6 {
            return Some(BgmPc::Top);
        }
        let (left, right, vol) = (self.bgm.spu_left, self.bgm.spu_right, self.bgm.volume as u16);
        self.io.sd_set_addr(BGM_LEFT | 0x2040, left);
        self.io.sd_set_addr(BGM_RIGHT | 0x2040, right);
        self.io.sd_set_param(BGM_LEFT | 0x200, 0xeb3);
        self.io.sd_set_param(BGM_RIGHT | 0x200, 0xeb3);
        self.io.sd_set_param(BGM_LEFT, vol);
        self.io.sd_set_param(BGM_LEFT | 0x100, 0);
        self.io.sd_set_param(BGM_RIGHT, 0);
        self.io.sd_set_param(BGM_RIGHT | 0x100, vol);
        self.io.sd_set_switch(0x1501, 0xc00000);
        if self.bgm.state != 6 {
            self.bgm.state = 7;
        }
        Some(BgmPc::Top)
    }

    fn bgm_read(&mut self, half: i32) -> i32 {
        let mut buf = std::mem::take(&mut self.bgm.buf[half as usize]);
        let r = self.cd_read(&mut buf);
        self.bgm.buf[half as usize] = buf;
        if r < 0x20000 { -1 } else { 0 }
    }

    /// The loop flags go into the ADPCM blocks before they leave: the first
    /// half starts the loop on both channels, the second half's last block
    /// jumps back. Both channels of a half go as two transfers; the right one
    /// is started from `bgm_transfer_done` once the left one is done, and a
    /// transfer is done on return.
    fn bgm_transfer(&mut self, half: i32) {
        self.bgm.xfer_half = half;
        if half == 0 {
            let b = &mut self.bgm.buf[0];
            b[1] = 6;
            b[0x11] = 2;
            b[0x10001] = 6;
            b[0x10011] = 2;
        } else {
            let b = &mut self.bgm.buf[1];
            b[0xfff1] = 3;
            b[0x1fff1] = 3;
        }
        self.bgm.xfer = 1;
        let at = (half as u32).wrapping_mul(0x10000).wrapping_add(self.bgm.spu_left);
        let data = self.bgm.buf[half as usize][..BGM_HALF].to_vec();
        self.io.spu.write(at, &data);
    }

    fn bgm_transfer_done(&mut self) -> bool {
        if self.bgm.xfer == 0 {
            return true;
        }
        if self.bgm.xfer == 1 {
            self.bgm.xfer = 2;
            let half = self.bgm.xfer_half;
            let at = (half as u32).wrapping_mul(0x10000).wrapping_add(self.bgm.spu_right);
            let data = self.bgm.buf[half as usize][BGM_HALF..].to_vec();
            self.io.spu.write(at, &data);
            return false;
        }
        true
    }

    // ---- the BGM's file on the disc ------------------------------------------------
    // One file open at a time through CDSTREAM, by sector: fd 0 is the only fd.
    // Names are upper-cased and given ISO 9660's `\NAME;1` form.

    fn cd_open(&mut self, name: &str) -> i32 {
        let path: String = format!("\\{name};1").chars().map(|c| if c == '/' { '\\' } else { c.to_ascii_uppercase() }).collect();
        match self.disc.search(&path) {
            Some((lsn, size)) => {
                self.bgm.cd_lsn = lsn;
                self.bgm.cd_size = size;
                self.bgm.cd_sector = lsn;
                0
            }
            None => -1,
        }
    }

    fn cd_read(&mut self, buf: &mut [u8]) -> i32 {
        let size = buf.len() as u32;
        let b = &self.bgm;
        if b.fd != 0 {
            return -1;
        }
        if b.cd_sector.wrapping_sub(b.cd_lsn).wrapping_mul(0x800) >= b.cd_size {
            return 0;
        }
        let sector = b.cd_sector;
        if !self.disc.read(sector, size / 0x800, buf) {
            buf.fill(0);
        }
        let b = &mut self.bgm;
        b.cd_sector = b.cd_sector.wrapping_add(size / 0x800);
        if b.cd_sector.wrapping_sub(b.cd_lsn).wrapping_mul(0x800) > b.cd_size {
            return b.cd_size.wrapping_sub(b.cd_sector.wrapping_sub(1).wrapping_sub(b.cd_lsn).wrapping_mul(0x800)) as i32;
        }
        size as i32
    }
}
